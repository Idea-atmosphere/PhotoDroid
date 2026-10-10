//! Reading what the user picked. This runs on a worker thread, never on the UI thread, so a large
//! document doesn't freeze the window while it's read.

use std::io::{self, Read};

/// Read all of `reader`, with the buffer sized up front from `size_hint` (the file's length), so
/// a large file isn't copied again while the buffer grows. Memory that isn't there is an error
/// the user sees, not an abort.
pub fn read_to_bytes(reader: &mut impl Read, size_hint: u64) -> io::Result<Vec<u8>> {
    let want = usize::try_from(size_hint).unwrap_or(0);
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(want)
        .map_err(|e| io::Error::other(format!("not enough memory for a {want}-byte file: {e}")))?;
    reader.read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::read_to_bytes;
    use std::io::Cursor;

    fn sample(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn reads_the_whole_input_with_its_length_as_the_hint() {
        let data = sample(300_000);
        assert_eq!(read_to_bytes(&mut Cursor::new(data.clone()), data.len() as u64).unwrap(), data);
    }

    #[test]
    fn an_unknown_length_still_reads_everything() {
        let data = sample(70_000);
        assert_eq!(read_to_bytes(&mut Cursor::new(data.clone()), 0).unwrap(), data);
    }

    #[test]
    fn a_hint_larger_than_memory_is_an_error_not_an_abort() {
        assert!(read_to_bytes(&mut Cursor::new(vec![1, 2, 3]), u64::MAX).is_err());
    }

    #[test]
    fn an_empty_input_is_an_empty_document() {
        assert_eq!(read_to_bytes(&mut Cursor::new(Vec::new()), 0).unwrap(), Vec::<u8>::new());
    }
}
