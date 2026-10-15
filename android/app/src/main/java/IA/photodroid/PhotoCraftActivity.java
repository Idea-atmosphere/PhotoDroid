package IA.photodroid;

import android.app.Dialog;
import android.app.NativeActivity;
import android.content.ContentResolver;
import android.content.ContentValues;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
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

import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
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
    private static final String DONATE_URL = "https://idea-atmosphere.github.io/Nothing/designs/minimal.html";

    private static volatile boolean welcomeShownInProcess = false;
    private Dialog welcomeDialog;

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

        // Show the welcome & donation popup on startup unless permanently dismissed.
        window.getDecorView().post(this::maybeShowWelcomePopup);
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
