use gk_server::search::model::paragraphize;

#[test]
fn paragraphize_empty() {
    let spans = paragraphize("");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].start, 0);
}

#[test]
fn paragraphize_short() {
    let text = "Line one.\nLine two.\nLine three.\n";
    let spans = paragraphize(text);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].highlight, text);
}

#[test]
fn paragraphize_long() {
    // Create 25 lines of text, each ~60 chars
    let text: String = (0..25)
        .map(|i| format!("This is sentence number {} with some extra padding text.\n", i))
        .collect();
    let spans = paragraphize(&text);
    assert!(spans.len() > 1, "expected multiple spans, got {}", spans.len());
    // Every character should be covered by at least one span
    let min_start = spans.iter().map(|s| s.start).min().unwrap();
    let max_end = spans.iter().map(|s| s.end).max().unwrap();
    assert_eq!(min_start, 0);
    assert_eq!(max_end, text.len());
}
