package IA.photodroid;

import android.content.Context;
import android.text.Editable;
import android.text.InputType;
import android.view.KeyEvent;
import android.view.View;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;

/**
 * A one-pixel, transparent editor used only as Android's IME endpoint. PhotoCraft renders its real
 * editors with egui, so the connection forwards committed text and editing keys to the native
 * event queue instead of displaying or editing a second document here.
 *
 * The BaseInputConnection editable is only an IME-session mirror. Composing text stays in that
 * mirror until commit/finish; this prevents an uncommitted Persian (or other IME) preedit from
 * being inserted repeatedly into egui.
 */
final class PhotoCraftIme extends View {
    private final PhotoCraftActivity activity;

    PhotoCraftIme(Context context, PhotoCraftActivity activity) {
        super(context);
        this.activity = activity;
        setFocusable(true);
        setFocusableInTouchMode(true);
        setAlpha(0.0f);
        setBackgroundColor(0x00000000);
        setWillNotDraw(true);
    }

    /** True only while this view is the active editor, not while native key input owns the window. */
    boolean servingKeyboard() {
        return getVisibility() == VISIBLE && hasFocus();
    }

    @Override
    public boolean onCheckIsTextEditor() {
        return true;
    }

    @Override
    public InputConnection onCreateInputConnection(EditorInfo outAttrs) {
        outAttrs.inputType = InputType.TYPE_CLASS_TEXT
                | InputType.TYPE_TEXT_FLAG_MULTI_LINE
                | InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS;
        outAttrs.imeOptions = EditorInfo.IME_ACTION_DONE
                | EditorInfo.IME_FLAG_NO_EXTRACT_UI
                | EditorInfo.IME_FLAG_NO_FULLSCREEN;
        outAttrs.initialSelStart = 0;
        outAttrs.initialSelEnd = 0;
        activity.keyboardStatus("InputConnection created");
        return new Connection(this);
    }

    private final class Connection extends BaseInputConnection {
        Connection(View targetView) {
            super(targetView, true);
        }

        @Override
        public boolean commitText(CharSequence text, int newCursorPosition) {
            String value = text == null ? "" : text.toString();
            super.commitText(text, newCursorPosition);
            if (!value.isEmpty()) {
                activity.queueKeyboardText("InputConnection.commitText", value);
            } else {
                activity.keyboardStatus("InputConnection.commitText empty");
            }
            return true;
        }

        @Override
        public boolean setComposingText(CharSequence text, int newCursorPosition) {
            // Keep the preedit in the local mirror. Only its final commit is forwarded, so a
            // sequence of replacements cannot duplicate text in the real egui editor.
            activity.keyboardStatus("InputConnection.setComposingText");
            return super.setComposingText(text, newCursorPosition);
        }

        @Override
        public boolean finishComposingText() {
            // Some IMEs finish rather than explicitly commit. Preserve that final preedit once.
            String pending = composingText();
            super.finishComposingText();
            if (!pending.isEmpty()) {
                activity.queueKeyboardText("InputConnection.finishComposingText", pending);
            } else {
                activity.keyboardStatus("InputConnection.finishComposingText empty");
            }
            return true;
        }

        @Override
        public boolean deleteSurroundingText(int beforeLength, int afterLength) {
            boolean composing = !composingText().isEmpty();
            super.deleteSurroundingText(beforeLength, afterLength);
            if (!composing) {
                activity.queueDeletion(beforeLength, afterLength);
            } else {
                // The preedit was never inserted into egui, so deleting it must not delete a real
                // character there. The local mirror above still reflects what the IME expects.
                activity.keyboardStatus("InputConnection.deleteSurroundingText composing-only");
            }
            return true;
        }

        @Override
        public boolean deleteSurroundingTextInCodePoints(int beforeLength, int afterLength) {
            boolean composing = !composingText().isEmpty();
            super.deleteSurroundingTextInCodePoints(beforeLength, afterLength);
            if (!composing) {
                activity.queueDeletion(beforeLength, afterLength);
            } else {
                activity.keyboardStatus("InputConnection.deleteSurroundingTextInCodePoints composing-only");
            }
            return true;
        }

        @Override
        public boolean sendKeyEvent(KeyEvent event) {
            if (activity.queueKeyEvent(event)) {
                return true;
            }
            activity.keyboardStatus("InputConnection.sendKeyEvent fallback keyCode=" + event.getKeyCode());
            return super.sendKeyEvent(event);
        }

        @Override
        public boolean performEditorAction(int actionCode) {
            activity.keyboardStatus("InputConnection.performEditorAction action=" + actionCode);
            activity.queueKeyPress("Enter");
            return true;
        }

        @Override
        public boolean requestCursorUpdates(int cursorUpdateMode) {
            // The real caret belongs to egui; this transparent view has no meaningful rectangle.
            return false;
        }

        private String composingText() {
            Editable editable = getEditable();
            if (editable == null) {
                return "";
            }
            int start = BaseInputConnection.getComposingSpanStart(editable);
            int end = BaseInputConnection.getComposingSpanEnd(editable);
            if (start < 0 || end < 0 || start == end) {
                return "";
            }
            int from = Math.max(0, Math.min(start, end));
            int to = Math.min(editable.length(), Math.max(start, end));
            return from < to ? editable.subSequence(from, to).toString() : "";
        }
    }
}
