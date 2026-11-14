//! Regression test for RUSTSEC-2026-0187 in the test-only PDF dependency.

fn pdf_with_nested_arrays(depth: usize) -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let catalog_offset = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /X ");
    pdf.extend(std::iter::repeat_n(b'[', depth));
    pdf.extend(std::iter::repeat_n(b']', depth));
    pdf.extend_from_slice(b" >>\nendobj\n");
    let xref_offset = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 2\n0000000000 65535 f \n{catalog_offset:010} 00000 n \ntrailer\n<< /Size 2 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n")
            .as_bytes(),
    );
    pdf
}

#[test]
fn lopdf_limits_deeply_nested_objects_without_aborting() {
    let normal = lopdf::Document::load_mem(&pdf_with_nested_arrays(3)).expect("shallow PDF should load");
    assert!(normal.catalog().and_then(|catalog| catalog.get(b"X")).is_ok());

    // The advisory's reproducer aborts vulnerable versions via stack overflow. Fixed lopdf may
    // return an error or skip the invalid object and return a document without a usable catalog.
    if let Ok(deep) = lopdf::Document::load_mem(&pdf_with_nested_arrays(10_380)) {
        assert!(deep.catalog().is_err(), "the deeply nested catalog must not be accepted");
    }
}
