package IA.photodroid;

import android.app.Dialog;
import android.app.NativeActivity;
import android.content.ContentResolver;
import android.content.ContentValues;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageInfo;
import android.content.res.Resources;
import android.database.Cursor;
import android.graphics.Color;
import android.graphics.Insets;
import android.graphics.Typeface;
import android.graphics.drawable.ColorDrawable;
import android.graphics.drawable.GradientDrawable;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.Environment;
import android.os.LocaleList;
import android.os.ParcelFileDescriptor;
import android.provider.MediaStore;
import android.provider.OpenableColumns;
import android.util.DisplayMetrics;
import android.util.TypedValue;
import android.view.DisplayCutout;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup;
import android.view.Window;
import android.view.WindowInsets;
import android.view.WindowManager;
import android.view.inputmethod.InputMethodManager;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.BufferedReader;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.concurrent.ConcurrentLinkedQueue;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.atomic.AtomicLong;

/**
 * The Android side of PhotoCraft: a NativeActivity whose native code is the Rust library named in
 * the manifest (android.app.lib_name). The Rust shell calls {@link #photocraftCall} for each
 * request, and polls it for events. The Rust side of the protocol is apps/photocraft-android/src/
 * protocol.rs.
 *
 * Protocol. Arguments and replies are strings whose fields are separated by U+001F.
 *   request  command + argument
 *   reply    "ok", "ok" + SEP + payload, or "err" + SEP + message
 *   events   "fd" SEP name SEP descriptor  (a picked file the native side opens and takes over)
 *            "file" SEP name SEP cachePath  (a picked file copied to app cache, when the provider
 *                                           gave no descriptor)
 *            "error" SEP name SEP message   (a picked file that couldn't be copied)
 *            "done"                         (the picker session is over)
 *            one event per "poll"; "" when none is waiting
 */
public class PhotoCraftActivity extends NativeActivity {
    private static final char SEP = '\u001f';
    private static final int REQUEST_OPEN = 0x5043;
    private static final String EXPORT_FOLDER = Environment.DIRECTORY_DOWNLOADS + "/PhotoCraft";
    private static final String PREFS_NAME = "photodroid_prefs";
    private static final String KEY_DONT_SHOW_WELCOME = "dont_show_donate_dialog";
    private static final String KEY_SKIPPED_UPDATE_TAG = "skipped_update_tag";
    private static final String KEY_LAST_SEEN_VERSION = "last_seen_version";
    private static final String DONATE_URL = "https://idea-atmosphere.github.io/Nothing/designs/minimal.html";
    private static final String GITHUB_LATEST_RELEASE_API = "https://api.github.com/repos/Idea-atmosphere/PhotoDroid/releases/latest";
    private static final String GITHUB_RELEASES_LIST_API = "https://api.github.com/repos/Idea-atmosphere/PhotoDroid/releases?per_page=1";
    private static final String GITHUB_RELEASES_PAGE = "https://github.com/Idea-atmosphere/PhotoDroid/releases";

    private static volatile boolean welcomeShownInProcess = false;
    private static volatile boolean autoUpdateCheckedInProcess = false;
    private Dialog welcomeDialog;
    private Dialog updateDialog;

    private final ConcurrentLinkedQueue<String> events = new ConcurrentLinkedQueue<>();
    private final ExecutorService io = Executors.newSingleThreadExecutor(runnable -> {
        Thread thread = new Thread(runnable, "photocraft-io");
        thread.setDaemon(true);
        return thread;
    });
    private final AtomicLong copies = new AtomicLong();

    // The system bars and the soft keyboard, in device pixels. Written on the UI thread, read by
    // the Rust shell (see the "insets" request).
    private volatile int insetTop;
    private volatile int insetBottom;
    private volatile int insetLeft;
    private volatile int insetRight;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        Window window = getWindow();
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            // Use the notch on the short edges too; the insets below include it.
            WindowManager.LayoutParams params = window.getAttributes();
            params.layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
            window.setAttributes(params);
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            // Edge to edge: the surface covers the system bars, and the keyboard doesn't resize the
            // window. The Rust side lays the UI out inside the insets reported here instead.
            window.setDecorFitsSystemWindows(false);
            window.setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_ADJUST_NOTHING);
        } else {
            // Before Android 11 the window is laid out between the bars already, so only the
            // notch needs reporting, and the keyboard resizes the window.
            window.setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE);
        }
        window.getDecorView().setOnApplyWindowInsetsListener((view, insets) -> {
            recordInsets(insets);
            return insets;
        });
        // Picked files that an earlier run didn't get to are stale now.
        deleteTree(new File(getCacheDir(), "open"));

        // Show the welcome & donation popup on startup unless permanently dismissed,
        // and check for updates / announce new version changes.
        window.getDecorView().post(() -> {
            maybeShowWelcomePopup();
            if (!autoUpdateCheckedInProcess) {
                autoUpdateCheckedInProcess = true;
                checkUpdateAsync(false);
            }
        });
    }

    @Override
    protected void onDestroy() {
        if (welcomeDialog != null && welcomeDialog.isShowing()) {
            try {
                welcomeDialog.dismiss();
            } catch (RuntimeException ignored) {
            }
        }
        welcomeDialog = null;
        if (updateDialog != null && updateDialog.isShowing()) {
            try {
                updateDialog.dismiss();
            } catch (RuntimeException ignored) {
            }
        }
        updateDialog = null;
        io.shutdownNow();
        super.onDestroy();
    }

    // ---- Startup welcome & donation popup ----

    private boolean isPersianDeviceLocale() {
        Locale locale = null;
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
            LocaleList sysLocales = Resources.getSystem().getConfiguration().getLocales();
            if (!sysLocales.isEmpty()) {
                locale = sysLocales.get(0);
            }
        }
        if (locale == null) {
            locale = Locale.getDefault();
        }
        if (locale == null) {
            return false;
        }
        String lang = locale.getLanguage();
        return lang != null && lang.toLowerCase(Locale.ROOT).startsWith("fa");
    }

    private int dp(int value) {
        return Math.round(TypedValue.applyDimension(
                TypedValue.COMPLEX_UNIT_DIP,
                value,
                getResources().getDisplayMetrics()
        ));
    }

    private void maybeShowWelcomePopup() {
        if (isFinishing() || isDestroyed() || welcomeShownInProcess) {
            return;
        }
        SharedPreferences prefs = getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE);
        if (prefs.getBoolean(KEY_DONT_SHOW_WELCOME, false)) {
            return;
        }
        welcomeShownInProcess = true;

        final boolean isFa = isPersianDeviceLocale();
        final String titleText = "PhotoDroid";
        final String bodyText = isFa
                ? "این برنامه پورت‌شده‌ی غیررسمیِ پروژه PhotoCraft هست برای اندروید.\n\n"
                  + "این برنامه کاملاً رایگان هست و اگر دوست داشتید می‌تونید دونیتم کنید و از این لینک، لینک دونیت از دارامت (برای کاربران داخل ایران) و آدرس کیف پول گذاشته شده:"
                : "This app is an unofficial Android port of the PhotoCraft project.\n\n"
                  + "This app is completely free, and if you'd like, you can donate to support me. Through this link, both a Daramet donation link (for users in Iran) and wallet addresses are provided:";
        final String linkCardTitle = isFa
                ? "لینک دونیت (دارامت و آدرس کیف پول)"
                : "Donate (Daramet & Wallet Address)";
        final String thanksText = isFa
                ? "از همراهی شما ممنونم."
                : "Thank you for your support.";
        final String closeLabel = isFa ? "بستن" : "Close";
        final String dontShowLabel = isFa ? "دیگر نمایش نده" : "Don't show again";

        Dialog dialog = new Dialog(this);
        dialog.requestWindowFeature(Window.FEATURE_NO_TITLE);
        dialog.setCancelable(true);
        dialog.setCanceledOnTouchOutside(true);

        LinearLayout card = new LinearLayout(this);
        card.setOrientation(LinearLayout.VERTICAL);
        card.setLayoutDirection(isFa ? View.LAYOUT_DIRECTION_RTL : View.LAYOUT_DIRECTION_LTR);
        int pad = dp(22);
        card.setPadding(pad, pad, pad, pad);

        GradientDrawable cardBg = new GradientDrawable();
        cardBg.setColor(Color.parseColor("#181B22"));
        cardBg.setCornerRadius(dp(24));
        cardBg.setStroke(dp(1), Color.parseColor("#2D3342"));
        card.setBackground(cardBg);

        // Header title
        TextView titleView = new TextView(this);
        titleView.setText(titleText);
        titleView.setTextColor(Color.parseColor("#F3F4F6"));
        titleView.setTextSize(TypedValue.COMPLEX_UNIT_SP, 20);
        titleView.setTypeface(Typeface.DEFAULT_BOLD);
        titleView.setGravity(isFa ? Gravity.RIGHT : Gravity.LEFT);
        card.addView(titleView, new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT
        ));

        // Scrollable content area for small screens / landscape mode
        ScrollView scrollView = new ScrollView(this);
        LinearLayout.LayoutParams scrollParams = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                0,
                1.0f
        );
        scrollParams.topMargin = dp(12);
        scrollParams.bottomMargin = dp(16);

        LinearLayout contentLayout = new LinearLayout(this);
        contentLayout.setOrientation(LinearLayout.VERTICAL);

        // Main description message
        TextView bodyView = new TextView(this);
        bodyView.setText(bodyText);
        bodyView.setTextColor(Color.parseColor("#D1D5DB"));
        bodyView.setTextSize(TypedValue.COMPLEX_UNIT_SP, 15);
        bodyView.setLineSpacing(dp(4), 1.15f);
        bodyView.setGravity(isFa ? Gravity.RIGHT : Gravity.LEFT);
        bodyView.setTextDirection(isFa ? View.TEXT_DIRECTION_RTL : View.TEXT_DIRECTION_LTR);
        contentLayout.addView(bodyView, new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT
        ));

        // Clickable Donation Link Box
        LinearLayout linkBox = new LinearLayout(this);
        linkBox.setOrientation(LinearLayout.VERTICAL);
        int boxPadH = dp(16);
        int boxPadV = dp(14);
        linkBox.setPadding(boxPadH, boxPadV, boxPadH, boxPadV);
        GradientDrawable linkBoxBg = new GradientDrawable();
        linkBoxBg.setColor(Color.parseColor("#222838"));
        linkBoxBg.setCornerRadius(dp(16));
        linkBoxBg.setStroke(dp(1), Color.parseColor("#3B6CF6"));
        linkBox.setBackground(linkBoxBg);
        linkBox.setClickable(true);
        linkBox.setFocusable(true);
        linkBox.setOnClickListener(v -> openUrl(DONATE_URL));

        TextView linkHeader = new TextView(this);
        linkHeader.setText(linkCardTitle);
        linkHeader.setTextColor(Color.parseColor("#6B8CFF"));
        linkHeader.setTextSize(TypedValue.COMPLEX_UNIT_SP, 14);
        linkHeader.setTypeface(Typeface.DEFAULT_BOLD);
        linkHeader.setGravity(isFa ? Gravity.RIGHT : Gravity.LEFT);
        linkBox.addView(linkHeader, new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT
        ));

        TextView linkUrlView = new TextView(this);
        linkUrlView.setText(DONATE_URL);
        linkUrlView.setTextColor(Color.parseColor("#F3F4F6"));
        linkUrlView.setTextSize(TypedValue.COMPLEX_UNIT_SP, 13);
        linkUrlView.setGravity(Gravity.LEFT);
        linkUrlView.setTextDirection(View.TEXT_DIRECTION_LTR);
        LinearLayout.LayoutParams urlParams = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT
        );
        urlParams.topMargin = dp(4);
        linkBox.addView(linkUrlView, urlParams);

        LinearLayout.LayoutParams linkBoxParams = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT
        );
        linkBoxParams.topMargin = dp(14);
        linkBoxParams.bottomMargin = dp(14);
        contentLayout.addView(linkBox, linkBoxParams);

        // Closing thank-you line
        TextView thanksView = new TextView(this);
        thanksView.setText(thanksText);
        thanksView.setTextColor(Color.parseColor("#F3F4F6"));
        thanksView.setTextSize(TypedValue.COMPLEX_UNIT_SP, 15);
        thanksView.setTypeface(Typeface.DEFAULT_BOLD);
        thanksView.setGravity(isFa ? Gravity.RIGHT : Gravity.LEFT);
        thanksView.setTextDirection(isFa ? View.TEXT_DIRECTION_RTL : View.TEXT_DIRECTION_LTR);
        contentLayout.addView(thanksView, new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT
        ));

        scrollView.addView(contentLayout);
        card.addView(scrollView, scrollParams);

        // Action buttons row (Close / Don't show again)
        LinearLayout buttonRow = new LinearLayout(this);
        buttonRow.setOrientation(LinearLayout.HORIZONTAL);
        buttonRow.setGravity(Gravity.CENTER_VERTICAL);

        TextView closeBtn = createDialogButton(
                closeLabel,
                Color.parseColor("#262B38"),
                Color.parseColor("#383F52"),
                Color.parseColor("#E5E7EB")
        );
        closeBtn.setOnClickListener(v -> dialog.dismiss());

        TextView dontShowBtn = createDialogButton(
                dontShowLabel,
                Color.parseColor("#3B6CF6"),
                Color.parseColor("#3B6CF6"),
                Color.parseColor("#FFFFFF")
        );
        dontShowBtn.setOnClickListener(v -> {
            prefs.edit().putBoolean(KEY_DONT_SHOW_WELCOME, true).apply();
            dialog.dismiss();
        });

        LinearLayout.LayoutParams btnParams1 = new LinearLayout.LayoutParams(
                0,
                ViewGroup.LayoutParams.WRAP_CONTENT,
                1.0f
        );
        LinearLayout.LayoutParams btnParams2 = new LinearLayout.LayoutParams(
                0,
                ViewGroup.LayoutParams.WRAP_CONTENT,
                1.0f
        );
        if (isFa) {
            btnParams1.leftMargin = dp(6);
            btnParams2.rightMargin = dp(6);
        } else {
            btnParams1.rightMargin = dp(6);
            btnParams2.leftMargin = dp(6);
        }
        buttonRow.addView(closeBtn, btnParams1);
        buttonRow.addView(dontShowBtn, btnParams2);

        card.addView(buttonRow, new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT
        ));

        dialog.setContentView(card);
        Window dlgWindow = dialog.getWindow();
        if (dlgWindow != null) {
            dlgWindow.setBackgroundDrawable(new ColorDrawable(Color.TRANSPARENT));
            DisplayMetrics dm = getResources().getDisplayMetrics();
            int maxWidth = Math.min(dm.widthPixels - dp(36), dp(440));
            dlgWindow.setLayout(maxWidth, ViewGroup.LayoutParams.WRAP_CONTENT);
        }

        welcomeDialog = dialog;
        try {
            dialog.show();
        } catch (RuntimeException ignored) {
        }
    }

    private TextView createDialogButton(String label, int bgColor, int strokeColor, int textColor) {
        TextView btn = new TextView(this);
        btn.setText(label);
        btn.setTextColor(textColor);
        btn.setTextSize(TypedValue.COMPLEX_UNIT_SP, 14);
        btn.setTypeface(Typeface.DEFAULT_BOLD);
        btn.setGravity(Gravity.CENTER);
        int padV = dp(11);
        int padH = dp(12);
        btn.setPadding(padH, padV, padH, padV);
        GradientDrawable bg = new GradientDrawable();
        bg.setColor(bgColor);
        bg.setCornerRadius(dp(14));
        bg.setStroke(dp(1), strokeColor);
        btn.setBackground(bg);
        btn.setClickable(true);
        btn.setFocusable(true);
        return btn;
    }

    // ---- Update checker & Changelog popup ----

    private String currentAppVersion() {
        try {
            PackageInfo info = getPackageManager().getPackageInfo(getPackageName(), 0);
            if (info != null && info.versionName != null && !info.versionName.trim().isEmpty()) {
                return cleanVersion(info.versionName);
            }
        } catch (Exception ignored) {
        }
        return "0.9.0";
    }

    private static String cleanVersion(String raw) {
        if (raw == null) {
            return "0.0.0";
        }
        String s = raw.trim();
        while (s.startsWith("v") || s.startsWith("V")) {
            s = s.substring(1).trim();
        }
        return s.isEmpty() ? "0.0.0" : s;
    }

    /**
     * Compares two semantic version strings (e.g. "0.9.1" vs "0.9.0").
     * Returns > 0 if a > b, < 0 if a < b, 0 if equal.
     */
    private static int compareVersions(String a, String b) {
        String[] pa = cleanVersion(a).split("[^0-9]+");
        String[] pb = cleanVersion(b).split("[^0-9]+");
        int len = Math.max(pa.length, pb.length);
        for (int i = 0; i < len; i++) {
            int va = 0;
            int vb = 0;
            if (i < pa.length && !pa[i].isEmpty()) {
                try {
                    va = Integer.parseInt(pa[i]);
                } catch (NumberFormatException ignored) {
                }
            }
            if (i < pb.length && !pb[i].isEmpty()) {
                try {
                    vb = Integer.parseInt(pb[i]);
                } catch (NumberFormatException ignored) {
                }
            }
            if (va != vb) {
                return Integer.compare(va, vb);
            }
        }
        return 0;
    }

    private static String currentVersionChangelog(boolean isFa) {
        if (isFa) {
            return "• همگام‌سازی کامل با هسته و امکانات جدید PhotoCraft v0.9.0\n"
                 + "• رفع مشکل نگه داشتن طولانی (Long-press) دکمه‌های نوار ابزار پایین برای باز شدن زیرمجموعهٔ ابزارها (مثل Lasso، Polygonal Lasso و Magnetic Lasso)\n"
                 + "• افزودن بخش دیباگ (Debug) در انتهای منوی همبرگری برای شروع ذخیرهٔ خودکار فایل‌های لاگ .txt هنگام روشن بودن و حذف کامل آن‌ها هنگام خاموش کردن\n"
                 + "• افزودن بخش انتخاب تعداد هسته‌های فعال پردازنده (CPU Cores) در انتهای منوی همبرگری\n"
                 + "• افزودن سیستم بررسی خودکار و دستی آپدیت جدید به همراه نمایش لیست تغییرات";
        }
        return "• Full sync with upstream PhotoCraft v0.9.0 engine & UI features\n"
             + "• Fixed long-pressing bottom toolbar buttons to open sub-tool flyout menus (e.g. Lasso, Polygonal Lasso, Magnetic Lasso)\n"
             + "• Added Debug toggle at the bottom of the hamburger menu to start saving .txt logs when enabled and delete them when disabled\n"
             + "• Added Active CPU Cores selector at the bottom of the hamburger menu\n"
             + "• Added automatic & manual Update Checker with release changelog display";
    }

    private static final class ReleaseInfo {
        final String tag;
        final String name;
        final String body;
        final String downloadUrl;

        ReleaseInfo(String tag, String name, String body, String downloadUrl) {
            this.tag = tag;
            this.name = name;
            this.body = body;
            this.downloadUrl = downloadUrl;
        }
    }

    private static ReleaseInfo fetchJsonRelease(String apiUrl) {
        HttpURLConnection conn = null;
        try {
            URL url = new URL(apiUrl);
            conn = (HttpURLConnection) url.openConnection();
            conn.setRequestMethod("GET");
            conn.setConnectTimeout(7000);
            conn.setReadTimeout(7000);
            conn.setRequestProperty("Accept", "application/vnd.github+json");
            conn.setRequestProperty("User-Agent", "PhotoDroid-UpdateChecker");
            int code = conn.getResponseCode();
            if (code < 200 || code >= 300) {
                return null;
            }
            StringBuilder sb = new StringBuilder();
            try (BufferedReader reader = new BufferedReader(
                    new InputStreamReader(conn.getInputStream(), StandardCharsets.UTF_8))) {
                String line;
                while ((line = reader.readLine()) != null) {
                    sb.append(line).append('\n');
                }
            }
            String raw = sb.toString().trim();
            JSONObject obj;
            if (raw.startsWith("[")) {
                JSONArray arr = new JSONArray(raw);
                if (arr.length() == 0) {
                    return null;
                }
                obj = arr.getJSONObject(0);
            } else {
                obj = new JSONObject(raw);
            }
            String tag = obj.optString("tag_name", "").trim();
            if (tag.isEmpty()) {
                return null;
            }
            String name = obj.optString("name", tag).trim();
            String body = obj.optString("body", "").trim();
            String htmlUrl = obj.optString("html_url", GITHUB_RELEASES_PAGE).trim();
            String apkUrl = "";
            JSONArray assets = obj.optJSONArray("assets");
            if (assets != null) {
                for (int i = 0; i < assets.length(); i++) {
                    JSONObject asset = assets.optJSONObject(i);
                    if (asset != null) {
                        String assetName = asset.optString("name", "");
                        String dl = asset.optString("browser_download_url", "");
                        if (assetName.endsWith(".apk") && !dl.isEmpty()) {
                            apkUrl = dl;
                            break;
                        }
                    }
                }
            }
            String targetUrl = !apkUrl.isEmpty() ? apkUrl : (htmlUrl.isEmpty() ? GITHUB_RELEASES_PAGE : htmlUrl);
            return new ReleaseInfo(tag, name, body, targetUrl);
        } catch (Exception ignored) {
            return null;
        } finally {
            if (conn != null) {
                conn.disconnect();
            }
        }
    }

    private static ReleaseInfo fetchLatestRelease() {
        ReleaseInfo latest = fetchJsonRelease(GITHUB_LATEST_RELEASE_API);
        if (latest != null) {
            return latest;
        }
        return fetchJsonRelease(GITHUB_RELEASES_LIST_API);
    }

    private void checkUpdateAsync(final boolean manual) {
        final String currentVer = currentAppVersion();
        final SharedPreferences prefs = getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE);
        final String lastSeenVer = prefs.getString(KEY_LAST_SEEN_VERSION, "");
        final String skippedTag = prefs.getString(KEY_SKIPPED_UPDATE_TAG, "");

        Thread worker = new Thread(() -> {
            final ReleaseInfo info = fetchLatestRelease();
            runOnUiThread(() -> {
                if (isFinishing() || isDestroyed()) {
                    return;
                }
                boolean hasNewerRemote = info != null && compareVersions(info.tag, currentVer) > 0;
                if (hasNewerRemote) {
                    if (!manual && cleanVersion(info.tag).equals(cleanVersion(skippedTag))) {
                        return;
                    }
                    prefs.edit().putString(KEY_LAST_SEEN_VERSION, currentVer).apply();
                    showUpdateDialog(
                            true,
                            currentVer,
                            cleanVersion(info.tag),
                            info.name,
                            info.body,
                            info.downloadUrl,
                            manual
                    );
                } else if (manual) {
                    prefs.edit().putString(KEY_LAST_SEEN_VERSION, currentVer).apply();
                    String notes = (info != null && !info.body.isEmpty()) ? info.body : "";
                    String dl = (info != null && !info.downloadUrl.isEmpty()) ? info.downloadUrl : GITHUB_RELEASES_PAGE;
                    showUpdateDialog(
                            false,
                            currentVer,
                            currentVer,
                            "PhotoDroid v" + currentVer,
                            notes,
                            dl,
                            true
                    );
                } else if (!currentVer.equals(lastSeenVer)) {
                    // First launch after installing or updating to this version: announce the new version & its changes.
                    prefs.edit().putString(KEY_LAST_SEEN_VERSION, currentVer).apply();
                    String notes = (info != null && !info.body.isEmpty() && compareVersions(info.tag, currentVer) == 0)
                            ? info.body
                            : "";
                    showUpdateDialog(
                            false,
                            currentVer,
                            currentVer,
                            "PhotoDroid v" + currentVer,
                            notes,
                            GITHUB_RELEASES_PAGE,
                            false
                    );
                }
            });
        }, "photodroid-update-check");
        worker.setDaemon(true);
        worker.start();
    }

    private void showUpdateDialog(
            final boolean updateAvailable,
            final String currentVer,
            final String targetVer,
            final String releaseTitle,
            final String remoteNotes,
            final String downloadUrl,
            final boolean manual
    ) {
        if (isFinishing() || isDestroyed()) {
            return;
        }
        if (updateDialog != null && updateDialog.isShowing()) {
            try {
                updateDialog.dismiss();
            } catch (RuntimeException ignored) {
            }
        }

        final boolean isFa = isPersianDeviceLocale();
        final SharedPreferences prefs = getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE);

        final String headerText;
        final String versionSubtitle;
        if (updateAvailable) {
            headerText = isFa ? "آپدیت جدید PhotoDroid منتشر شد! 🎉" : "New PhotoDroid Update Available! 🎉";
            versionSubtitle = isFa
                    ? ("نسخه فعلی: v" + currentVer + "  ←  نسخه جدید: v" + targetVer)
                    : ("Current: v" + currentVer + "  →  New: v" + targetVer);
        } else if (manual) {
            headerText = isFa ? "برنامه به‌روز است (نسخه v" + currentVer + ")" : "You're Up to Date (v" + currentVer + ")";
            versionSubtitle = isFa
                    ? "شما در حال استفاده از آخرین نسخهٔ منتشرشده هستید."
                    : "You are running the latest version of PhotoDroid.";
        } else {
            headerText = isFa ? "به نسخهٔ جدید PhotoDroid v" + currentVer + " خوش آمدید!" : "What's New in PhotoDroid v" + currentVer;
            versionSubtitle = isFa
                    ? "نسخهٔ جدید با موفقیت نصب شد. خلاصه تغییرات این آپدیت:"
                    : "Updated successfully. Here is what changed in this version:";
        }

        final String changelogHeader = isFa ? "تغییرات و ویژگی‌های نسخه:" : "Changelog & What's New:";
        final String builtInNotes = currentVersionChangelog(isFa);
        final String combinedNotes;
        if (remoteNotes != null && !remoteNotes.trim().isEmpty()) {
            combinedNotes = updateAvailable
                    ? remoteNotes.trim()
                    : (builtInNotes + "\n\n— GitHub Release Notes —\n" + remoteNotes.trim());
        } else {
            combinedNotes = builtInNotes;
        }

        Dialog dialog = new Dialog(this);
        dialog.requestWindowFeature(Window.FEATURE_NO_TITLE);
        dialog.setCancelable(true);
        dialog.setCanceledOnTouchOutside(true);

        LinearLayout card = new LinearLayout(this);
        card.setOrientation(LinearLayout.VERTICAL);
        card.setLayoutDirection(isFa ? View.LAYOUT_DIRECTION_RTL : View.LAYOUT_DIRECTION_LTR);
        int pad = dp(22);
        card.setPadding(pad, pad, pad, pad);

        GradientDrawable cardBg = new GradientDrawable();
        cardBg.setColor(Color.parseColor("#181B22"));
        cardBg.setCornerRadius(dp(24));
        cardBg.setStroke(dp(1), Color.parseColor(updateAvailable ? "#3B6CF6" : "#2D3342"));
        card.setBackground(cardBg);

        TextView titleView = new TextView(this);
        titleView.setText(headerText);
        titleView.setTextColor(Color.parseColor("#F3F4F6"));
        titleView.setTextSize(TypedValue.COMPLEX_UNIT_SP, 18);
        titleView.setTypeface(Typeface.DEFAULT_BOLD);
        titleView.setGravity(isFa ? Gravity.RIGHT : Gravity.LEFT);
        card.addView(titleView, new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT
        ));

        TextView subView = new TextView(this);
        subView.setText(versionSubtitle);
        subView.setTextColor(Color.parseColor("#6B8CFF"));
        subView.setTextSize(TypedValue.COMPLEX_UNIT_SP, 13);
        subView.setTypeface(Typeface.DEFAULT_BOLD);
        subView.setGravity(isFa ? Gravity.RIGHT : Gravity.LEFT);
        LinearLayout.LayoutParams subParams = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT
        );
        subParams.topMargin = dp(6);
        card.addView(subView, subParams);

        ScrollView scrollView = new ScrollView(this);
        LinearLayout.LayoutParams scrollParams = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                0,
                1.0f
        );
        scrollParams.topMargin = dp(12);
        scrollParams.bottomMargin = dp(16);

        LinearLayout notesBox = new LinearLayout(this);
        notesBox.setOrientation(LinearLayout.VERTICAL);
        int boxPad = dp(14);
        notesBox.setPadding(boxPad, boxPad, boxPad, boxPad);
        GradientDrawable notesBg = new GradientDrawable();
        notesBg.setColor(Color.parseColor("#222838"));
        notesBg.setCornerRadius(dp(16));
        notesBg.setStroke(dp(1), Color.parseColor("#2E364A"));
        notesBox.setBackground(notesBg);

        TextView notesTitle = new TextView(this);
        notesTitle.setText(changelogHeader + (releaseTitle != null && !releaseTitle.isEmpty() ? " (" + releaseTitle + ")" : ""));
        notesTitle.setTextColor(Color.parseColor("#F3F4F6"));
        notesTitle.setTextSize(TypedValue.COMPLEX_UNIT_SP, 14);
        notesTitle.setTypeface(Typeface.DEFAULT_BOLD);
        notesTitle.setGravity(isFa ? Gravity.RIGHT : Gravity.LEFT);
        notesBox.addView(notesTitle, new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT
        ));

        TextView notesBody = new TextView(this);
        notesBody.setText(combinedNotes);
        notesBody.setTextColor(Color.parseColor("#D1D5DB"));
        notesBody.setTextSize(TypedValue.COMPLEX_UNIT_SP, 14);
        notesBody.setLineSpacing(dp(4), 1.15f);
        notesBody.setGravity(isFa ? Gravity.RIGHT : Gravity.LEFT);
        notesBody.setTextDirection(isFa ? View.TEXT_DIRECTION_RTL : View.TEXT_DIRECTION_LTR);
        LinearLayout.LayoutParams bodyParams = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT
        );
        bodyParams.topMargin = dp(8);
        notesBox.addView(notesBody, bodyParams);

        scrollView.addView(notesBox);
        card.addView(scrollView, scrollParams);

        LinearLayout buttonRow = new LinearLayout(this);
        buttonRow.setOrientation(LinearLayout.HORIZONTAL);
        buttonRow.setGravity(Gravity.CENTER_VERTICAL);

        final String closeLabel = updateAvailable
                ? (isFa ? "بعداً" : "Later")
                : (isFa ? "بستن" : "Close");
        final String primaryLabel = updateAvailable
                ? (isFa ? "دریافت آپدیت (APK)" : "Download Update")
                : (isFa ? "صفحه نسخه‌ها در گیت‌هاب" : "GitHub Releases");

        TextView secondaryBtn = createDialogButton(
                closeLabel,
                Color.parseColor("#262B38"),
                Color.parseColor("#383F52"),
                Color.parseColor("#E5E7EB")
        );
        secondaryBtn.setOnClickListener(v -> dialog.dismiss());

        TextView primaryBtn = createDialogButton(
                primaryLabel,
                Color.parseColor("#3B6CF6"),
                Color.parseColor("#3B6CF6"),
                Color.parseColor("#FFFFFF")
        );
        primaryBtn.setOnClickListener(v -> {
            openUrl(downloadUrl != null && !downloadUrl.isEmpty() ? downloadUrl : GITHUB_RELEASES_PAGE);
            dialog.dismiss();
        });

        LinearLayout.LayoutParams btnParams1 = new LinearLayout.LayoutParams(
                0,
                ViewGroup.LayoutParams.WRAP_CONTENT,
                1.0f
        );
        LinearLayout.LayoutParams btnParams2 = new LinearLayout.LayoutParams(
                0,
                ViewGroup.LayoutParams.WRAP_CONTENT,
                1.0f
        );
        if (isFa) {
            btnParams1.leftMargin = dp(6);
            btnParams2.rightMargin = dp(6);
        } else {
            btnParams1.rightMargin = dp(6);
            btnParams2.leftMargin = dp(6);
        }
        buttonRow.addView(secondaryBtn, btnParams1);
        buttonRow.addView(primaryBtn, btnParams2);

        card.addView(buttonRow, new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT
        ));

        if (updateAvailable && !manual) {
            TextView skipBtn = new TextView(this);
            skipBtn.setText(isFa ? "نادیده گرفتن این نسخه" : "Skip this version");
            skipBtn.setTextColor(Color.parseColor("#9CA3AF"));
            skipBtn.setTextSize(TypedValue.COMPLEX_UNIT_SP, 12);
            skipBtn.setGravity(Gravity.CENTER);
            skipBtn.setPadding(dp(8), dp(10), dp(8), dp(2));
            skipBtn.setOnClickListener(v -> {
                prefs.edit().putString(KEY_SKIPPED_UPDATE_TAG, targetVer).apply();
                dialog.dismiss();
            });
            card.addView(skipBtn, new LinearLayout.LayoutParams(
                    ViewGroup.LayoutParams.MATCH_PARENT,
                    ViewGroup.LayoutParams.WRAP_CONTENT
            ));
        }

        dialog.setContentView(card);
        Window dlgWindow = dialog.getWindow();
        if (dlgWindow != null) {
            dlgWindow.setBackgroundDrawable(new ColorDrawable(Color.TRANSPARENT));
            DisplayMetrics dm = getResources().getDisplayMetrics();
            int maxWidth = Math.min(dm.widthPixels - dp(36), dp(440));
            dlgWindow.setLayout(maxWidth, ViewGroup.LayoutParams.WRAP_CONTENT);
        }

        updateDialog = dialog;
        try {
            dialog.show();
        } catch (RuntimeException ignored) {
        }
    }

    /** Called from Rust. Never throws: failures come back as an "err" reply. */
    public String photocraftCall(String command, String argument) {
        try {
            switch (command) {
                case "open":
                    launchOpen();
                    return ok("");
                case "poll": {
                    String event = events.poll();
                    return event == null ? "" : event;
                }
                case "insets":
                    return ok(insetTop + "," + insetBottom + "," + insetLeft + "," + insetRight);
                case "save":
                    return ok(save(argument));
                case "delete_logs":
                    return ok(deleteLogs(argument));
                case "check_update":
                    checkUpdateAsync(true);
                    return ok("");
                case "url":
                    return openUrl(argument);
                case "keyboard":
                    softKeyboard("show".equals(argument));
                    return ok("");
                default:
                    return err("unknown request " + clean(command));
            }
        } catch (Exception e) {
            return err(describe(e));
        }
    }

    // ---- Opening files (the Storage Access Framework picker) ----

    @SuppressWarnings("deprecation")
    private void launchOpen() {
        runOnUiThread(() -> {
            Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            intent.setType("*/*");
            intent.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true);
            try {
                startActivityForResult(intent, REQUEST_OPEN);
            } catch (RuntimeException e) {
                events.add(join("error", "", "No file picker is available: " + describe(e)));
                events.add("done");
            }
        });
    }

    @Override
    @SuppressWarnings("deprecation")
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode != REQUEST_OPEN) {
            return;
        }
        List<Uri> picked = new ArrayList<>();
        if (resultCode == RESULT_OK && data != null) {
            if (data.getClipData() != null) {
                for (int i = 0; i < data.getClipData().getItemCount(); i++) {
                    picked.add(data.getClipData().getItemAt(i).getUri());
                }
            } else if (data.getData() != null) {
                picked.add(data.getData());
            }
        }
        io.execute(() -> {
            for (Uri uri : picked) {
                importPicked(uri);
            }
            events.add("done");
        });
    }

    /**
     * Hand one picked document to the native side. When the provider gives a descriptor, the
     * native side reads the file itself, so nothing is copied; otherwise the file is copied into
     * app cache first. Runs on the io thread.
     */
    private void importPicked(Uri uri) {
        ContentResolver resolver = getContentResolver();
        String name = displayName(resolver, uri);
        int fd = openDescriptor(resolver, uri);
        if (fd >= 0) {
            events.add(join("fd", name, Integer.toString(fd)));
            return;
        }
        try {
            File dir = new File(getCacheDir(), "open");
            if (!dir.isDirectory() && !dir.mkdirs()) {
                throw new IOException("could not create " + dir.getName());
            }
            File target = new File(dir, Long.toString(copies.incrementAndGet()));
            try (InputStream in = resolver.openInputStream(uri)) {
                if (in == null) {
                    throw new IOException("the file could not be opened");
                }
                try (OutputStream out = new FileOutputStream(target)) {
                    copy(in, out);
                }
            }
            events.add(join("file", name, target.getAbsolutePath()));
        } catch (Exception e) {
            events.add(join("error", name, describe(e)));
        }
    }

    /** The picked file as a descriptor the native side takes over, or -1 if the provider has none. */
    private static int openDescriptor(ContentResolver resolver, Uri uri) {
        try {
            ParcelFileDescriptor pfd = resolver.openFileDescriptor(uri, "r");
            return pfd == null ? -1 : pfd.detachFd();
        } catch (Exception e) {
            return -1;
        }
    }

    private static String displayName(ContentResolver resolver, Uri uri) {
        String name = null;
        try (Cursor cursor = resolver.query(uri, new String[] {OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (cursor != null && cursor.moveToFirst()) {
                name = cursor.getString(0);
            }
        } catch (RuntimeException ignored) {
            // The provider doesn't say; the last path segment is the fallback.
        }
        if (name == null || name.isEmpty()) {
            name = uri.getLastPathSegment();
        }
        return fileName(name == null || name.isEmpty() ? "picked file" : name);
    }

    // ---- Saving (Downloads/PhotoCraft through MediaStore) ----

    /**
     * Save the staged file under its name in Downloads/PhotoCraft. The argument is
     * name SEP mime SEP stagedPath SEP previousUri, where previousUri is the copy this name was
     * saved to last (empty if none). Returns the new copy's URI.
     */
    private String save(String argument) throws IOException {
        String[] fields = argument.split(String.valueOf(SEP), -1);
        if (fields.length != 4) {
            throw new IOException("malformed save request");
        }
        String name = fields[0];
        String mime = fields[1];
        String staged = fields[2];
        String previous = fields[3];

        ContentResolver resolver = getContentResolver();
        ContentValues pending = new ContentValues();
        pending.put(MediaStore.Downloads.DISPLAY_NAME, name);
        pending.put(MediaStore.Downloads.MIME_TYPE, mime);
        pending.put(MediaStore.Downloads.RELATIVE_PATH, EXPORT_FOLDER);
        pending.put(MediaStore.Downloads.IS_PENDING, 1);
        Uri row = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, pending);
        if (row == null) {
            throw new IOException("Downloads is not available");
        }
        try (InputStream in = new FileInputStream(staged);
             OutputStream out = resolver.openOutputStream(row, "wt")) {
            if (out == null) {
                throw new IOException("could not write " + name);
            }
            copy(in, out);
        } catch (IOException | RuntimeException e) {
            resolver.delete(row, null, null);
            throw e;
        }

        // The new copy is complete, so the earlier export of this name can go. Only then does the
        // new copy take the plain name (while the old one exists, MediaStore numbers it).
        if (!previous.isEmpty()) {
            try {
                resolver.delete(Uri.parse(previous), null, null);
            } catch (RuntimeException ignored) {
                // Already gone (the user deleted it): nothing to replace.
            }
        }
        ContentValues published = new ContentValues();
        published.put(MediaStore.Downloads.IS_PENDING, 0);
        if (resolver.update(row, published, null, null) < 1) {
            throw new IOException("could not finish " + name);
        }
        ContentValues renamed = new ContentValues();
        renamed.put(MediaStore.Downloads.DISPLAY_NAME, name);
        try {
            resolver.update(row, renamed, null, null);
        } catch (RuntimeException ignored) {
            // The file is saved, under MediaStore's numbered name.
        }
        return row.toString();
    }

    /**
     * Removes any saved diagnostic log (.txt) files from Downloads/PhotoCraft when Debug is turned off.
     */
    private String deleteLogs(String argument) {
        ContentResolver resolver = getContentResolver();
        if (argument != null && !argument.isEmpty()) {
            String[] uris = argument.split(String.valueOf(SEP), -1);
            for (String uri : uris) {
                if (uri != null && !uri.isEmpty()) {
                    try {
                        resolver.delete(Uri.parse(uri), null, null);
                    } catch (RuntimeException ignored) {
                    }
                }
            }
        }
        try {
            resolver.delete(
                    MediaStore.Downloads.EXTERNAL_CONTENT_URI,
                    MediaStore.Downloads.DISPLAY_NAME + " LIKE ?",
                    new String[] {"photocraft-log%"}
            );
        } catch (RuntimeException ignored) {
        }
        try {
            File downloads = Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS);
            if (downloads != null) {
                File folder = new File(downloads, "PhotoCraft");
                File[] children = folder.listFiles();
                if (children != null) {
                    for (File child : children) {
                        String n = child.getName();
                        if (n != null && n.startsWith("photocraft-log") && n.endsWith(".txt")) {
                            //noinspection ResultOfMethodCallIgnored
                            child.delete();
                        }
                    }
                }
            }
        } catch (RuntimeException ignored) {
        }
        return "";
    }

    // ---- Links ----

    private String openUrl(String url) {
        if (!url.startsWith("https://") && !url.startsWith("http://")) {
            return err("only web links can be opened");
        }
        runOnUiThread(() -> {
            try {
                startActivity(new Intent(Intent.ACTION_VIEW, Uri.parse(url)));
            } catch (RuntimeException e) {
                events.add(join("error", "", "The link could not be opened: " + describe(e)));
            }
        });
        return ok("");
    }

    // ---- System bars and keyboard ----

    private void recordInsets(WindowInsets insets) {
        int top;
        int bottom;
        int left;
        int right;
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            Insets bars = insets.getInsets(WindowInsets.Type.systemBars() | WindowInsets.Type.displayCutout());
            Insets keyboard = insets.getInsets(WindowInsets.Type.ime());
            top = bars.top;
            left = bars.left;
            right = bars.right;
            bottom = Math.max(bars.bottom, keyboard.bottom);
        } else {
            top = 0;
            bottom = 0;
            left = 0;
            right = 0;
            DisplayCutout cutout = insets.getDisplayCutout();
            if (cutout != null) {
                top = cutout.getSafeInsetTop();
                bottom = cutout.getSafeInsetBottom();
                left = cutout.getSafeInsetLeft();
                right = cutout.getSafeInsetRight();
            }
        }
        insetTop = top;
        insetBottom = bottom;
        insetLeft = left;
        insetRight = right;
    }

    /**
     * Shows or hides the soft keyboard for text being edited. The request is explicit (flags 0).
     * winit's own request is implicit, and Android may ignore an implicit request once the user has
     * dismissed the keyboard, so typing would not bring it back. For the same reason winit's hide,
     * which only takes away a keyboard shown implicitly, can't be relied on to hide this one.
     */
    private void softKeyboard(boolean show) {
        runOnUiThread(() -> {
            InputMethodManager imm = (InputMethodManager) getSystemService(Context.INPUT_METHOD_SERVICE);
            if (imm == null) {
                return;
            }
            View view = getWindow().getDecorView();
            if (show) {
                imm.showSoftInput(view, 0);
            } else {
                imm.hideSoftInputFromWindow(view.getWindowToken(), 0);
            }
        });
    }

    // ---- Helpers ----

    private static void copy(InputStream in, OutputStream out) throws IOException {
        byte[] buffer = new byte[1 << 16];
        int read;
        while ((read = in.read(buffer)) > 0) {
            out.write(buffer, 0, read);
        }
    }

    private static void deleteTree(File file) {
        File[] children = file.listFiles();
        if (children != null) {
            for (File child : children) {
                deleteTree(child);
            }
        }
        // Deleting is best effort: a file that stays is only cache.
        //noinspection ResultOfMethodCallIgnored
        file.delete();
    }

    /** Replace the field separator and control characters, so a string can't break the protocol. */
    private static String clean(String text) {
        StringBuilder out = new StringBuilder(text.length());
        for (int i = 0; i < text.length(); i++) {
            char c = text.charAt(i);
            out.append(c == SEP || Character.isISOControl(c) ? '_' : c);
        }
        return out.toString();
    }

    /** A display name that is also a safe file name: no separators either. */
    private static String fileName(String text) {
        return clean(text).replace('/', '_').replace('\\', '_');
    }

    private static String describe(Throwable t) {
        String message = t.getMessage();
        return t.getClass().getSimpleName() + (message == null ? "" : ": " + clean(message));
    }

    private static String join(String... fields) {
        StringBuilder out = new StringBuilder();
        for (int i = 0; i < fields.length; i++) {
            if (i > 0) {
                out.append(SEP);
            }
            out.append(clean(fields[i]));
        }
        return out.toString();
    }

    private static String ok(String payload) {
        return payload.isEmpty() ? "ok" : "ok" + SEP + payload;
    }

    private static String err(String message) {
        return "err" + SEP + clean(message);
    }
}
