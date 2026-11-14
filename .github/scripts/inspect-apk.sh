#!/usr/bin/env bash
# Usage: inspect-apk.sh <apk> <ndk-llvm-bin-dir> <aapt2>
#
# Checks that the APK can work at runtime, without a device: the native library is packaged for
# arm64-v8a and exports the entry points the activity needs, and the Java side calls the method
# the library calls. The results go out as workflow notices. Exits non-zero if a check fails.
set -u

apk=$1
llvm=$2
aapt2=$3
work=$(mktemp -d)
failed=0

escape() { sed -e 's/%/%25/g' -e 's/\r/%0D/g' | awk '{ printf "%s%%0A", $0 }'; }
notice() { echo "::notice title=$1::$(escape)"; }
check() { # check <title> <command...>
  local title=$1; shift
  if "$@" > "$work/out" 2>&1; then
    echo "PASS $title"
  else
    echo "::error title=APK check failed::$title"
    failed=1
  fi
}

# What the package says about itself (names, SDK levels, ABIs, launcher activity).
"$aapt2" dump badging "$apk" 2>&1 | grep -E "^package|sdkVersion|targetSdkVersion|native-code|launchable-activity" | notice "APK badging"

# The manifest as the APK ships it (the lib_name that makes the activity load our library).
"$aapt2" dump xmltree --file AndroidManifest.xml "$apk" 2>&1 | grep -E "lib_name|photocraft_android|PhotoCraftActivity" | notice "APK manifest"

unzip -l "$apk" | grep -E "lib/|classes\.dex|resources\.arsc" | awk '{ print $4, $1 }' | notice "APK entries"

so="$work/libphotocraft_android.so"
check "library packaged for arm64-v8a" unzip -l "$apk" lib/arm64-v8a/libphotocraft_android.so
unzip -p "$apk" lib/arm64-v8a/libphotocraft_android.so > "$so" 2>/dev/null
check "library is an AArch64 shared object" bash -c "file '$so' | grep -q 'ARM aarch64'"

exports=$("$llvm/llvm-nm" -D --defined-only "$so" 2>/dev/null)
for symbol in android_main ANativeActivity_onCreate; do
  if printf '%s\n' "$exports" | grep -q " $symbol\$"; then
    echo "PASS exports $symbol"
  else
    echo "::error title=APK check failed::the library doesn't export $symbol"
    failed=1
  fi
done
printf '%s\n' "$exports" | grep -E " (android_main|ANativeActivity_onCreate)\$" | notice "native entry points"

# The Java side must have the method the Rust side calls (photocraftCall). The dex files are
# split (classes.dex, classes2.dex, ...), so every one of them is searched.
found=0
for dex in $(unzip -Z1 "$apk" | grep -E '^classes[0-9]*\.dex$'); do
  unzip -p "$apk" "$dex" > "$work/$dex"
  if grep -a -q "photocraftCall" "$work/$dex"; then
    echo "PASS $dex has photocraftCall"
    found=1
  fi
done
if [ "$found" -ne 1 ]; then
  echo "::error title=APK check failed::no dex file has photocraftCall"
  failed=1
fi

rm -rf "$work"
exit "$failed"
