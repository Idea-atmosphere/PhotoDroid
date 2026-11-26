#!/usr/bin/env bash
# Host-only path/SAF policy checks; this does not simulate Android MediaStore or a device.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/src/IA/photodroid" "$tmp/classes"
cat > "$tmp/src/IA/photodroid/UserFoldersTest.java" <<'JAVA'
package IA.photodroid;

public final class UserFoldersTest {
    private static void check(boolean ok, String message) {
        if (!ok) throw new AssertionError(message);
    }
    public static void main(String[] args) {
        String downloads = "Download";
        check(UserFolders.NAME.equals("PhotoDroid"), "public folder name");
        check(UserFolders.root(downloads).equals("Download/PhotoDroid"), "root path");
        check(UserFolders.isRootDocumentId("primary:Download/PhotoDroid", downloads), "primary grant");
        check(UserFolders.isRootDocumentId("1234-5678:Download/PhotoDroid", downloads), "other volume grant");
        check(!UserFolders.isRootDocumentId(null, downloads), "null tree");
        check(!UserFolders.isRootDocumentId("primary:Download/PhotoCraft", downloads), "old folder grant");
        check(!UserFolders.isRootDocumentId("primary:Download", downloads), "parent grant");
        check(!UserFolders.isRootDocumentId("primary:Download/PhotoDroid/Fonts", downloads), "child grant");
        check(!UserFolders.isRootDocumentId("primary:Download/PhotoDroid2", downloads), "similar folder");
        System.out.println("UserFolders host policy checks passed");
    }
}
JAVA
javac -encoding UTF-8 -d "$tmp/classes" \
    "$root/android/app/src/main/java/IA/photodroid/UserFolders.java" \
    "$tmp/src/IA/photodroid/UserFoldersTest.java"
java -cp "$tmp/classes" IA.photodroid.UserFoldersTest
# Confirm the activity and Rust index actually consume the tested policy.
python3 - "$root" <<'PY'
from pathlib import Path
import re, sys
root = Path(sys.argv[1])
activity = (root / 'android/app/src/main/java/IA/photodroid/PhotoCraftActivity.java').read_text()
services = (root / 'apps/photocraft-android/src/services.rs').read_text()
assert 'EXPORT_FOLDER = UserFolders.root(Environment.DIRECTORY_DOWNLOADS)' in activity
for folder in ('PROJECT_FOLDER', 'OUTPUT_FOLDER', 'LOG_FOLDER', 'FONT_FOLDER'):
    assert re.search(rf'{folder} = EXPORT_FOLDER \+ "/', activity), folder
assert 'UserFolders.isRootDocumentId(id, Environment.DIRECTORY_DOWNLOADS)' in activity
assert 'new File(downloads, UserFolders.NAME)' in activity
assert 'self.root.join("exports-photodroid.txt")' in services
assert 'Saver::new(dirs.staging(), dirs.export_index(), link.save_targets())' in services
print('Activity paths and separate export index wiring checks passed')
PY
