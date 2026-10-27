use super::*;
use crate::*;

struct Provider;
impl CommandHandler for Provider {
    fn plan(
        &self,
        _: &EditIntent,
        _: &EditingState,
        _: &FeatureServices<'_>,
    ) -> Result<Option<CommandPlan>, String> {
        Ok(Some(CommandPlan::Consume))
    }
}
impl RenderActionProvider for Provider {
    fn id(&self) -> &'static str {
        "test-controls"
    }
    fn commands(&self) -> &'static [&'static str] {
        &[]
    }
    fn controls(&self, _: &EditingState) -> Result<Vec<RenderControl>, String> {
        Ok(Vec::new())
    }
    fn plan(&self, _: &EditingState, _: &RenderActionRequest) -> Result<CommandPlan, String> {
        Ok(CommandPlan::Consume)
    }
}
impl PreviewProvider for Provider {
    fn input(
        &self,
        _: &EditingState,
        _: &ryne_syntax::code_block::CodeBlock,
    ) -> Option<PreviewInput> {
        None
    }
}
impl BlockUiProvider for Provider {
    fn adornment(&self, block: &ryne_syntax::block::Block, _: bool) -> AdornmentSpec {
        AdornmentSpec {
            metadata: BlockMetadata::from(block.kind),
            menu: Vec::new(),
        }
    }
}
impl PresentationProvider for Provider {
    fn display(
        &self,
        state: &EditingState,
        _: &FeatureServices<'_>,
        _: Option<&mut dyn SyntaxHighlighter>,
        _: &[DiagramViewInput],
    ) -> Display {
        Display::plain_source(state.source.text())
    }
}

#[test]
fn rejected_duplicates_preserve_every_provider_and_staged_contribution() {
    let mut r = Registrar::default();
    let original = Arc::new(Provider);
    let rejected = Arc::new(Provider);
    macro_rules! check {
        ($method:ident, $field:ident, $slot:expr, $exclusive:expr) => {{
            let c = Contribution {
                id: stringify!($field),
                slot: $slot,
                exclusive: $exclusive,
                priority: 0,
            };
            r.$method(c.clone(), original.clone()).unwrap();
            let before = r.$field[c.id].clone();
            let staged = r.staged.len();
            assert!(r.$method(c.clone(), rejected.clone()).is_err());
            assert!(Arc::ptr_eq(&before, &r.$field[c.id]));
            assert_eq!(r.staged.len(), staged);
        }};
    }
    check!(command, commands, Slot::Command("edit"), false);
    check!(render_controls, controls, Slot::Ui("controls"), false);
    check!(preview, previews, Slot::Renderer("test"), true);
    check!(block_ui, block_ui, Slot::Ui("block-menu"), true);
    check!(
        presentation,
        presentations,
        Slot::PrimaryPresentation("document"),
        true
    );
    let c = Contribution {
        id: "templates",
        slot: Slot::InsertTemplate("test"),
        exclusive: false,
        priority: 0,
    };
    r.templates(c.clone(), vec![("original", InsertBlock::Quote)])
        .unwrap();
    let staged = r.staged.len();
    assert!(r
        .templates(c, vec![("rejected", InsertBlock::Paragraph)])
        .is_err());
    assert_eq!(r.templates["templates"][0].0, "original");
    assert_eq!(r.staged.len(), staged);
}

#[test]
fn recovering_from_a_duplicate_still_publishes_the_original_command() {
    struct Recover;
    impl FeatureModule for Recover {
        fn manifest(&self) -> FeatureManifest {
            FeatureManifest::builtin("recover")
        }
        fn register(&self, r: &mut Registrar) -> Result<(), String> {
            let c = Contribution {
                id: "recover.command",
                slot: Slot::Command("edit"),
                exclusive: false,
                priority: 0,
            };
            struct Reject;
            impl CommandHandler for Reject {
                fn plan(
                    &self,
                    _: &EditIntent,
                    _: &EditingState,
                    _: &FeatureServices<'_>,
                ) -> Result<Option<CommandPlan>, String> {
                    Err("rejected replacement executed".into())
                }
            }
            r.command(c.clone(), Arc::new(Provider))?;
            assert!(r.command(c, Arc::new(Reject)).is_err());
            Ok(())
        }
    }
    struct NoMath;
    impl FormulaRenderer for NoMath {
        fn render(&self, _: &str) -> Option<Arc<ryne_projection::Formula>> {
            None
        }
    }
    let registry = Registry::build(&[&Recover]).unwrap();
    let state = EditingState::new(editable_markdown_parser::Source::new("unchanged"));
    let services = FeatureServices {
        formulas: &NoMath,
        planning: Default::default(),
    };
    assert!(matches!(
        registry.plan(&EditIntent::Normalize, &state, &services),
        Ok(Some(CommandPlan::Consume))
    ));
    assert_eq!(registry.contributions().len(), 1);
    assert_eq!(state.source.text(), "unchanged");
}
