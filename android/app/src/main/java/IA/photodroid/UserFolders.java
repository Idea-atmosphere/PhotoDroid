package IA.photodroid;

/** Shared policy for PhotoDroid's public Downloads folder and its SAF grant. */
final class UserFolders {
    static final String NAME = "PhotoDroid";

    private UserFolders() {}

    static String root(String downloads) {
        return downloads + "/" + NAME;
    }

    /** Accept the chosen folder itself, on any storage volume, not its parent or descendants. */
    static boolean isRootDocumentId(String id, String downloads) {
        return id != null && id.endsWith(":" + root(downloads));
    }
}
