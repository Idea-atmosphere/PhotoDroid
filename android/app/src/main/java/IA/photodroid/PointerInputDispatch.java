package IA.photodroid;

import android.os.Build;
import android.view.InputDevice;
import android.view.View;

/**
 * Ask Android to deliver pointer input without waiting for batched input dispatch. The request is
 * made only when the decor view is attached; on API 29 the normal system delivery is unchanged.
 * This does not intercept events or change gesture handling. It applies to the pointer class,
 * including touch, stylus and mouse, and can increase event traffic while the app is in use.
 */
final class PointerInputDispatch {
    private PointerInputDispatch() {}

    static void request(View decorView) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R && decorView.isAttachedToWindow()) {
            decorView.requestUnbufferedDispatch(InputDevice.SOURCE_CLASS_POINTER);
        }
    }
}
