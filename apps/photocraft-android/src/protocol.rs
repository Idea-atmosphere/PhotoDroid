//! The text protocol between the Rust shell and `PhotoCraftActivity` (Java).
//!
//! Rust asks with `(command, argument)` strings. The activity answers `ok`, optionally followed by
//! a payload, or `err` followed by a message. What the user picked, and the end of each picker
//! session, arrive as events: one per `poll`, with fields separated by [`SEP`], and an empty reply
//! when none is waiting. Everything here is string handling, so it is tested on any host.

use std::collections::HashMap;

/// Separates the fields of a request, a reply or an event. It is a control character, which file
/// names and messages never contain (see [`file_name`]; the Java side replaces it too).
pub const SEP: char = '\u{1f}';

/// The longest file name kept, in characters.
const MAX_NAME_CHARS: usize = 200;

/// Something the activity reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A file the user picked, copied to app cache at `path`. The shell reads it and deletes it.
    File { name: String, path: String },
    /// A file the user picked, as an open descriptor. The shell takes ownership of it and reads it,
    /// so nothing is copied.
    Descriptor { name: String, fd: i32 },
    /// A picked file that couldn't be copied. `name` may be empty.
    Error { name: String, message: String },
    /// The picker session is over (after its files, if it returned any).
    Done,
}

/// Parse one event line. `None` for an unknown kind (and for an empty line).
pub fn parse_event(line: &str) -> Option<Event> {
    let mut fields = line.splitn(3, SEP);
    let kind = fields.next()?;
    let name = fields.next().unwrap_or_default().to_string();
    let rest = fields.next().unwrap_or_default().to_string();
    match kind {
        "file" => Some(Event::File { name, path: rest }),
        "fd" => match rest.parse::<i32>() {
            Ok(fd) if fd >= 0 => Some(Event::Descriptor { name, fd }),
            _ => Some(Event::Error { name, message: format!("invalid file descriptor {rest:?}") }),
        },
        "error" => Some(Event::Error { name, message: rest }),
        "done" => Some(Event::Done),
        _ => None,
    }
}

/// Parse a reply: `ok`, `ok SEP payload`, or `err SEP message`. Anything else is an error.
pub fn parse_reply(reply: &str) -> Result<String, String> {
    let (status, payload) = reply.split_once(SEP).unwrap_or((reply, ""));
    match status {
        "ok" => Ok(payload.to_string()),
        "err" if payload.is_empty() => Err("Android reported an error".to_string()),
        "err" => Err(payload.to_string()),
        _ => Err(format!("unexpected reply from Android: {reply:?}")),
    }
}

/// Join the fields of a request. A [`SEP`] inside a field becomes a space.
pub fn join_fields(fields: &[&str]) -> String {
    let separator = SEP.to_string();
    fields.iter().map(|field| field.replace(SEP, " ")).collect::<Vec<String>>().join(separator.as_str())
}

/// The file name a document is saved under: the last component of `path`, with control characters,
/// path separators and characters that storage providers reject replaced by `_`. Never empty.
pub fn file_name(path: &str) -> String {
    let base = path.rsplit(['/', '\\']).next().unwrap_or_default();
    let clean: String = base
        .chars()
        .map(|c| if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c })
        .take(MAX_NAME_CHARS)
        .collect();
    let trimmed = clean.trim().trim_matches('.');
    if trimmed.is_empty() { "PhotoCraft".to_string() } else { trimmed.to_string() }
}

/// The MIME type a saved file is filed under, from its extension.
pub fn mime_type(name: &str) -> &'static str {
    let extension = name.rsplit_once('.').map(|(_, ext)| ext.to_ascii_lowercase()).unwrap_or_default();
    match extension.as_str() {
        "psd" | "psb" => "image/vnd.adobe.photoshop",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "tif" | "tiff" => "image/tiff",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "csv" => "text/csv",
        "txt" => "text/plain",
        _ => "application/octet-stream",
    }
}

/// The `insets` payload: the top, bottom, left and right system insets in device pixels.
pub fn parse_insets(payload: &str) -> Option<[i32; 4]> {
    let values = payload.split(',').map(|value| value.trim().parse::<i32>().ok()).collect::<Option<Vec<i32>>>()?;
    match values.as_slice() {
        [top, bottom, left, right] => Some([*top, *bottom, *left, *right]),
        _ => None,
    }
}

/// Parse the index of saved exports (file name → the URI of the copy written last), which is kept
/// across launches so that saving the same name again replaces that copy. Malformed lines are
/// skipped.
pub fn parse_index(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| line.split_once(SEP))
        .map(|(name, uri)| (name.to_string(), uri.to_string()))
        .collect()
}

/// The text [`parse_index`] reads back.
pub fn format_index(index: &HashMap<String, String>) -> String {
    index.iter().map(|(name, uri)| format!("{name}{sep}{uri}\n", sep = SEP)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_parse_by_kind() {
        assert_eq!(
            parse_event(&join_fields(&["file", "cat.psd", "/data/cache/open/3"])),
            Some(Event::File { name: "cat.psd".into(), path: "/data/cache/open/3".into() })
        );
        assert_eq!(
            parse_event(&join_fields(&["error", "dog.jpg", "Permission denied"])),
            Some(Event::Error { name: "dog.jpg".into(), message: "Permission denied".into() })
        );
        assert_eq!(parse_event(&join_fields(&["error", "", "No file picker"])), Some(Event::Error { name: String::new(), message: "No file picker".into() }));
        assert_eq!(parse_event(&join_fields(&["fd", "cat.psd", "42"])), Some(Event::Descriptor { name: "cat.psd".into(), fd: 42 }));
        assert!(matches!(parse_event(&join_fields(&["fd", "x.psd", "-1"])), Some(Event::Error { .. })));
        assert!(matches!(parse_event(&join_fields(&["fd", "x.psd", "nope"])), Some(Event::Error { .. })));
        assert_eq!(parse_event("done"), Some(Event::Done));
        assert_eq!(parse_event(""), None);
        assert_eq!(parse_event(&join_fields(&["surprise", "x"])), None);
    }

    #[test]
    fn a_path_with_separators_keeps_them() {
        // Only the first two separators split; the rest of the message stays whole.
        let line = join_fields(&["error", "a.psd", "one"]) + &SEP.to_string() + "two";
        assert_eq!(parse_event(&line), Some(Event::Error { name: "a.psd".into(), message: format!("one{}two", SEP) }));
    }

    #[test]
    fn replies_are_ok_or_err() {
        assert_eq!(parse_reply("ok"), Ok(String::new()));
        assert_eq!(parse_reply(&join_fields(&["ok", "content://media/downloads/7"])), Ok("content://media/downloads/7".into()));
        assert_eq!(parse_reply(&join_fields(&["err", "disk full"])), Err("disk full".into()));
        assert_eq!(parse_reply("err"), Err("Android reported an error".into()));
        assert!(parse_reply("").is_err());
        assert!(parse_reply("maybe").is_err());
    }

    #[test]
    fn a_separator_in_a_field_cannot_split_it() {
        assert_eq!(join_fields(&["a\u{1f}b", "c"]), format!("a b{}c", SEP));
        assert_eq!(join_fields(&[]), "");
    }

    #[test]
    fn file_names_are_safe_for_the_media_store() {
        assert_eq!(file_name("/storage/emulated/0/Download/cat.psd"), "cat.psd");
        assert_eq!(file_name("C:\\photos\\x.png"), "x.png");
        assert_eq!(file_name("a:b*c?.png"), "a_b_c_.png");
        assert_eq!(file_name(&format!("bad{}name.psd", SEP)), "bad_name.psd");
        assert_eq!(file_name(""), "PhotoCraft");
        assert_eq!(file_name("..."), "PhotoCraft");
        assert_eq!(file_name("dir/"), "PhotoCraft");
        assert_eq!(file_name(&"x".repeat(500)).chars().count(), MAX_NAME_CHARS);
    }

    #[test]
    fn mime_types_follow_the_extension() {
        assert_eq!(mime_type("scan.PSD"), "image/vnd.adobe.photoshop");
        assert_eq!(mime_type("scan.jpeg"), "image/jpeg");
        assert_eq!(mime_type("notes.csv"), "text/csv");
        assert_eq!(mime_type("work.pcraft"), "application/octet-stream");
        assert_eq!(mime_type("noextension"), "application/octet-stream");
    }

    #[test]
    fn insets_parse_four_integers() {
        assert_eq!(parse_insets("0,96,0,0"), Some([0, 96, 0, 0]));
        assert_eq!(parse_insets(" 4 , 5 , 6 , 7 "), Some([4, 5, 6, 7]));
        assert_eq!(parse_insets("1,2,3"), None);
        assert_eq!(parse_insets("a,b,c,d"), None);
        assert_eq!(parse_insets(""), None);
    }

    #[test]
    fn the_export_index_round_trips() {
        let mut index = HashMap::new();
        index.insert("cat.psd".to_string(), "content://media/external_primary/downloads/7".to_string());
        index.insert("dog.png".to_string(), "content://media/external_primary/downloads/9".to_string());
        assert_eq!(parse_index(&format_index(&index)), index);
        // A damaged line is skipped, not fatal.
        assert_eq!(parse_index("no separator here\n"), HashMap::new());
    }
}
