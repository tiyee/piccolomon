use super::*;
struct TestFeature {
    manifest: FeatureManifest,
    slot: &'static str,
    fail: bool,
}
impl BuiltinFeature for TestFeature {
    fn manifest(&self) -> FeatureManifest {
        self.manifest.clone()
    }
    fn register(&self, r: &mut Registrar) -> Result<(), String> {
        r.contribute(Contribution {
            id: self.manifest.id,
            slot: Slot::Command(self.slot),
            exclusive: true,
            priority: 0,
        });
        if self.fail {
            Err("staged registration failed".into())
        } else {
            Ok(())
        }
    }
}
fn feature(id: &'static str, slot: &'static str) -> TestFeature {
    TestFeature {
        manifest: FeatureManifest::builtin(id),
        slot,
        fail: false,
    }
}
#[test]
fn dependency_and_exclusive_conflicts_fail_before_publication() {
    let a = feature("a", "edit");
    let b = feature("b", "edit");
    assert!(Registry::build(&[&a, &b]).is_err());
    assert!(Registry::build(&[&a, &a]).is_err());
    let mut b = feature("b", "other");
    b.manifest.requires.push("missing");
    assert!(Registry::build(&[&a, &b]).is_err());
    b.manifest.requires = vec!["a"];
    let mut a = feature("a", "edit");
    a.manifest.requires.push("b");
    assert!(Registry::build(&[&a, &b]).is_err());
    a.manifest.requires.clear();
    a.manifest.api = INTERNAL_API + 1;
    assert!(Registry::build(&[&a]).is_err());
    a.manifest.api = INTERNAL_API;
    a.fail = true;
    assert!(Registry::build(&[&a, &b]).is_err());
}
#[test]
fn explicit_order_is_deterministic_independent_of_registration_input() {
    let mut a = feature("a", "a");
    let z = feature("z", "z");
    a.manifest.after.push("z");
    let left = Registry::build(&[&a, &z]).unwrap();
    let right = Registry::build(&[&z, &a]).unwrap();
    assert_eq!(left.feature_order(), ["z", "a"]);
    assert_eq!(left.feature_order(), right.feature_order());
    let empty = Registry::build(&[]).unwrap();
    assert!(empty.feature_order().is_empty());
}

#[test]
fn slot_priority_respects_explicit_edges_without_alphabetical_precedence() {
    struct Decorator(FeatureManifest, i32);
    impl BuiltinFeature for Decorator {
        fn manifest(&self) -> FeatureManifest {
            self.0.clone()
        }
        fn register(&self, r: &mut Registrar) -> Result<(), String> {
            r.contribute(Contribution {
                id: self.0.id,
                slot: Slot::Decoration("text"),
                exclusive: false,
                priority: self.1,
            });
            Ok(())
        }
    }
    let a = Decorator(FeatureManifest::builtin("a"), 0);
    let mut b = Decorator(FeatureManifest::builtin("b"), 20);
    b.0.after.push("a");
    let z = Decorator(FeatureManifest::builtin("z"), 10);
    let registry = Registry::build(&[&b, &a, &z]).unwrap();
    assert_eq!(
        registry
            .contributions()
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
        ["z", "a", "b"]
    );
}

struct Executable {
    manifest: FeatureManifest,
    priority: i32,
    accept: bool,
    calls: std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>,
}
impl CommandHandler for Executable {
    fn plan(
        &self,
        _: &EditIntent,
        _: &EditingState,
        _: &FeatureServices<'_>,
    ) -> Result<Option<CommandPlan>, String> {
        self.calls.lock().unwrap().push(self.manifest.id);
        Ok(self.accept.then_some(CommandPlan::Consume))
    }
}
impl BuiltinFeature for Executable {
    fn manifest(&self) -> FeatureManifest {
        self.manifest.clone()
    }
    fn register(&self, r: &mut Registrar) -> Result<(), String> {
        r.command(
            Contribution {
                id: self.manifest.id,
                slot: Slot::InputRule("text"),
                priority: self.priority,
                exclusive: false,
            },
            std::sync::Arc::new(Self {
                manifest: self.manifest.clone(),
                priority: self.priority,
                accept: self.accept,
                calls: self.calls.clone(),
            }),
        )
    }
}
struct NoMath;
impl FormulaRenderer for NoMath {
    fn render(&self, _: &str) -> Option<std::sync::Arc<ryne_projection::Formula>> {
        None
    }
}
#[test]
fn execution_uses_validated_priority_dependencies_and_first_acceptance() {
    let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let a = Executable {
        manifest: FeatureManifest::builtin("a"),
        priority: 0,
        accept: false,
        calls: calls.clone(),
    };
    let mut b = Executable {
        manifest: FeatureManifest::builtin("b"),
        priority: 20,
        accept: true,
        calls: calls.clone(),
    };
    b.manifest.after.push("a");
    let z = Executable {
        manifest: FeatureManifest::builtin("z"),
        priority: 10,
        accept: false,
        calls: calls.clone(),
    };
    let state = EditingState::new(Source::new("unchanged"));
    let services = FeatureServices {
        formulas: &NoMath,
        planning: Default::default(),
    };
    for inputs in [[&a, &b, &z], [&z, &b, &a]] {
        calls.lock().unwrap().clear();
        let features: Vec<&dyn BuiltinFeature> = inputs
            .into_iter()
            .map(|f| f as &dyn BuiltinFeature)
            .collect();
        let registry = Registry::build(&features).unwrap();
        assert!(matches!(
            registry
                .plan(&EditIntent::Normalize, &state, &services)
                .unwrap(),
            Some(CommandPlan::Consume)
        ));
        assert_eq!(*calls.lock().unwrap(), ["z", "a", "b"]);
    }
    assert_eq!(state.source.text(), "unchanged");
}

#[test]
fn a_module_publishes_controls_templates_preview_and_ui_atomically() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    struct Bundle {
        calls: Arc<AtomicUsize>,
        fail: bool,
    }
    struct Controls(Arc<AtomicUsize>);
    impl RenderActionProvider for Controls {
        fn id(&self) -> &'static str {
            "bundle"
        }
        fn commands(&self) -> &'static [&'static str] {
            &["bundle.run"]
        }
        fn controls(&self, _: &EditingState) -> Result<Vec<RenderControl>, String> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Ok(Vec::new())
        }
        fn plan(&self, _: &EditingState, _: &RenderActionRequest) -> Result<CommandPlan, String> {
            Ok(CommandPlan::Consume)
        }
    }
    struct Preview;
    impl PreviewProvider for Preview {
        fn input(
            &self,
            _: &EditingState,
            block: &ryne_syntax::code_block::CodeBlock,
        ) -> Option<PreviewInput> {
            struct Renderer;
            impl DiagramRenderer for Renderer {
                fn render(&self, _: &str, _: CancelControl) -> DiagramOutcome {
                    Err("test fallback".into())
                }
                fn generation(&self) -> u64 {
                    0
                }
            }
            Some(PreviewInput {
                fold_errors: false,
                feature: "misreported",
                block: block.into(),
                input: Ok("registered".into()),
                renderer: Arc::new(Renderer),
            })
        }
    }
    struct Ui;
    impl BlockUiProvider for Ui {
        fn adornment(&self, block: &ryne_syntax::block::Block, _: bool) -> AdornmentSpec {
            AdornmentSpec {
                metadata: BlockMetadata::from(block.kind),
                menu: vec![MenuItemSpec {
                    command: BlockCommand::Copy,
                    name: "bundle menu",
                    order: 0,
                }],
            }
        }
    }
    impl FeatureModule for Bundle {
        fn manifest(&self) -> FeatureManifest {
            FeatureManifest::builtin("bundle")
        }
        fn register(&self, r: &mut Registrar) -> Result<(), String> {
            let c = |id, slot| Contribution {
                id,
                slot,
                exclusive: true,
                priority: 0,
            };
            r.render_controls(
                c("bundle.controls", Slot::Ui("controls")),
                Arc::new(Controls(self.calls.clone())),
            )?;
            r.preview(
                c("bundle.preview", Slot::Renderer("test")),
                Arc::new(Preview),
            )?;
            r.block_ui(c("bundle.ui", Slot::Ui("block-menu")), Arc::new(Ui))?;
            r.templates(
                c("bundle.template", Slot::InsertTemplate("test")),
                vec![("bundle", InsertBlock::Quote)],
            )?;
            if self.fail {
                Err("abort whole module".into())
            } else {
                Ok(())
            }
        }
    }
    let calls = Arc::new(AtomicUsize::new(0));
    let module = Bundle {
        calls: calls.clone(),
        fail: false,
    };
    let registry = Registry::build(&[&module]).unwrap();
    let state = EditingState::new(Source::new("```test\nbody\n```\n"));
    registry.render_actions().controls(&state).unwrap();
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(registry.insert_templates()[0].0, "bundle");
    assert_eq!(
        registry.preview_inputs(&state)[0].input.as_deref().unwrap(),
        "registered"
    );
    assert_eq!(registry.preview_inputs(&state)[0].feature, "bundle.preview");
    let block = ryne_syntax::SemanticQuery::new(&state.parse)
        .block_at(9)
        .unwrap();
    assert_eq!(registry.block_ui(&block, false).menu[0].name, "bundle menu");
    assert!(Registry::build(&[&module, &module]).is_err());
    assert!(Registry::build(&[&Bundle { calls, fail: true }]).is_err());
    // Failed builds never invalidate or partly replace the published registry.
    assert_eq!(registry.insert_templates().len(), 1);
}
