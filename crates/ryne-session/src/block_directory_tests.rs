use super::*;
use ryne_syntax::directory::BlockRole;

fn ids(doc: &EditorSession) -> Vec<ryne_syntax::directory::BlockId> {
    assert_eq!(doc.blocks().snapshot(), doc.source.id());
    doc.blocks()
        .entries(doc.source.id())
        .unwrap()
        .iter()
        .filter(|e| e.role == BlockRole::Paragraph)
        .map(|e| e.id)
        .collect()
}
#[test]
fn directory_publication_matches_commits_history_and_composition() {
    for newline in ["\n", "\r\n", "\r"] {
        let raw = format!("first{newline}{newline}second");
        let mut doc = regression_tests::new_document(&raw);
        let initial = ids(&doc);
        let before = doc.source.id();
        doc.select(2, false, false);
        doc.set_source_mode(true);
        assert_eq!(doc.blocks().snapshot(), before);
        assert_eq!(ids(&doc), initial);
        assert!(!doc.dirty());
        doc.insert_text(2..2, "😀").unwrap();
        assert_eq!(ids(&doc), initial);
        doc.try_undo(false).unwrap();
        assert_eq!(ids(&doc), initial);
        assert_eq!(doc.source.text(), raw);
        assert!(!doc.dirty());
        doc.try_undo(true).unwrap();
        let committed = doc.source.id();
        doc.update_composition(None, "候选", None).unwrap();
        assert_eq!(doc.blocks().snapshot(), committed);
        assert_eq!(ids(&doc), initial);
        doc.cancel_composition();
        assert_eq!(ids(&doc), initial);
    }
}
#[test]
fn rejected_candidates_do_not_publish_directory_or_history() {
    struct Reject;
    impl Validator for Reject {
        fn validate(&self, _: &EditingState, _: &CommandPlan) -> Result<(), String> {
            Err("no".into())
        }
    }
    let mut doc = regression_tests::new_document("first\n\nsecond");
    doc.insert_text(1..1, "x").unwrap();
    let snapshot = doc.source.id();
    let initial = ids(&doc);
    doc.add_validator(Arc::new(Reject));
    assert!(doc.insert_text(2..2, "y").is_err());
    assert!(doc.try_undo(false).is_err());
    assert_eq!(doc.blocks().snapshot(), snapshot);
    assert_eq!(ids(&doc), initial);
    assert_eq!(doc.history_len(false), 1);
    assert_eq!(doc.history_len(true), 0);
}

#[test]
fn deleted_identity_is_not_reused_by_history_reconstruction() {
    let mut doc = regression_tests::new_document("first\n\nsecond");
    let initial = ids(&doc);
    doc.replace(0..7, "").unwrap();
    assert_eq!(ids(&doc), initial[1..]);
    doc.try_undo(false).unwrap();
    assert_ne!(ids(&doc)[0], initial[0]);
    assert_eq!(ids(&doc)[1], initial[1]);
}
