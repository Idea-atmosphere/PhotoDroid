#!/usr/bin/env bash
# Host-only policy checks against small Android API fakes. This does not simulate Android input.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/src/android/os" "$tmp/src/android/view" "$tmp/src/IA/photodroid" "$tmp/classes"
cat > "$tmp/src/android/os/Build.java" <<'JAVA'
package android.os;
public final class Build {
    public static final class VERSION { public static int SDK_INT; }
    public static final class VERSION_CODES { public static final int R = 30; }
}
JAVA
cat > "$tmp/src/android/view/InputDevice.java" <<'JAVA'
package android.view;
public final class InputDevice { public static final int SOURCE_CLASS_POINTER = 2; }
JAVA
cat > "$tmp/src/android/view/View.java" <<'JAVA'
package android.view;
public class View {
    public boolean attached;
    public int calls;
    public int source = -1;
    public boolean isAttachedToWindow() { return attached; }
    public void requestUnbufferedDispatch(int value) { calls++; source = value; }
}
JAVA
cat > "$tmp/src/IA/photodroid/PointerInputDispatchTest.java" <<'JAVA'
package IA.photodroid;
import android.os.Build;
import android.view.InputDevice;
import android.view.View;
public final class PointerInputDispatchTest {
    private static void check(boolean value, String message) {
        if (!value) throw new AssertionError(message);
    }
    public static void main(String[] args) {
        View view = new View();
        view.attached = true;
        Build.VERSION.SDK_INT = 29;
        PointerInputDispatch.request(view);
        check(view.calls == 0, "API 29 must retain normal input dispatch");
        Build.VERSION.SDK_INT = 30;
        view.attached = false;
        PointerInputDispatch.request(view);
        check(view.calls == 0, "unattached view must not request dispatch");
        view.attached = true;
        PointerInputDispatch.request(view);
        check(view.calls == 1 && view.source == InputDevice.SOURCE_CLASS_POINTER,
              "attached API 30 view must request pointer-class dispatch");
        Build.VERSION.SDK_INT = 36;
        PointerInputDispatch.request(view);
        check(view.calls == 2 && view.source == InputDevice.SOURCE_CLASS_POINTER,
              "newer Android must use the same policy");
        System.out.println("PointerInputDispatch host policy checks passed");
    }
}
JAVA
javac -encoding UTF-8 -d "$tmp/classes" \
    "$tmp/src/android/os/Build.java" "$tmp/src/android/view/InputDevice.java" "$tmp/src/android/view/View.java" \
    "$root/android/app/src/main/java/IA/photodroid/PointerInputDispatch.java" \
    "$tmp/src/IA/photodroid/PointerInputDispatchTest.java"
java -cp "$tmp/classes" IA.photodroid.PointerInputDispatchTest
