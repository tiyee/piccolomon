use crate::commands::dispatch::BuiltinCommand;
use ryne_extensions::*;
use std::sync::Arc;
pub(crate) struct Builtin {
    pub id: &'static str,
    pub dependencies: &'static [&'static str],
    pub slot: Slot,
    pub formulas: Arc<dyn FormulaRenderer>,
}
impl BuiltinFeature for Builtin {
    fn manifest(&self) -> FeatureManifest {
        let mut manifest = FeatureManifest::builtin(self.id);
        manifest.requires = self.dependencies.to_vec();
        manifest
    }
    fn register(&self, r: &mut Registrar) -> Result<(), String> {
        r.contribute(Contribution {
            id: self.id,
            slot: self.slot.clone(),
            exclusive: true,
            priority: 0,
        });
        let handlers: &[BuiltinCommand] = match self.id {
            "code" => &[BuiltinCommand::Code],
            "list" => &[BuiltinCommand::List],
            "quote" => &[BuiltinCommand::Quote],
            "table" => &[BuiltinCommand::Table],
            "inline" => &[BuiltinCommand::Input],
            "paragraph-heading" => &[BuiltinCommand::Navigation, BuiltinCommand::Paragraph],
            "templates" => &[BuiltinCommand::Templates],
            "format" => &[BuiltinCommand::Format],
            _ => &[],
        };
        for &handler in handlers {
            r.command(
                Contribution {
                    id: handler.id(),
                    slot: if matches!(handler, BuiltinCommand::Input) {
                        Slot::InputRule("text")
                    } else {
                        Slot::Command("edit")
                    },
                    exclusive: false,
                    priority: handler.priority(),
                },
                Arc::new(handler),
            )?;
        }
        let contribution = |id, slot| Contribution {
            id,
            slot,
            exclusive: false,
            priority: 0,
        };
        match self.id {
            "code" => r.render_controls(
                contribution("code.controls", Slot::Ui("render-controls")),
                Arc::new(crate::code_actions::CodeActions {
                    formulas: self.formulas.clone(),
                }),
            )?,
            "table" => r.render_controls(
                contribution("table.controls", Slot::Ui("render-controls")),
                Arc::new(crate::table_actions::TableActions {
                    formulas: self.formulas.clone(),
                }),
            )?,
            "list" => r.render_controls(
                contribution("list.controls", Slot::Ui("render-controls")),
                Arc::new(crate::list_actions::ListActions {
                    formulas: self.formulas.clone(),
                }),
            )?,
            "templates" => r.templates(
                contribution("templates.items", Slot::InsertTemplate("items")),
                crate::templates::ITEMS.to_vec(),
            )?,
            "block-ui" => r.block_ui(
                Contribution {
                    exclusive: true,
                    ..contribution("blocks.menu", Slot::Ui("block-menu"))
                },
                Arc::new(crate::block_ui::MarkdownBlockUi),
            )?,
            _ => (),
        }
        if self.id == "document-presentation" {
            r.presentation(
                Contribution {
                    id: "document.display",
                    slot: Slot::PrimaryPresentation("document"),
                    exclusive: true,
                    priority: 0,
                },
                Arc::new(crate::presentation::MarkdownPresentation),
            )?;
        }
        Ok(())
    }
}
pub(crate) fn registry(
    formulas: &Arc<dyn FormulaRenderer>,
    previews: &[crate::diagrams::FencedPreview],
    images: Option<&crate::images::ImagePreview>,
    additional: &[Arc<dyn BuiltinFeature + Send + Sync>],
) -> Result<Registry, String> {
    let builtins = [
        Builtin {
            formulas: formulas.clone(),
            id: "format",
            dependencies: &[],
            slot: Slot::Command("format"),
        },
        Builtin {
            formulas: formulas.clone(),
            id: "document-presentation",
            dependencies: &["code", "table", "list", "quote", "formula"],
            slot: Slot::Ui("document-presentation"),
        },
        Builtin {
            formulas: formulas.clone(),
            id: "code",
            dependencies: &[],
            slot: Slot::PrimaryPresentation("code"),
        },
        Builtin {
            formulas: formulas.clone(),
            id: "list",
            dependencies: &[],
            slot: Slot::PrimaryPresentation("list"),
        },
        Builtin {
            formulas: formulas.clone(),
            id: "table",
            dependencies: &[],
            slot: Slot::PrimaryPresentation("table"),
        },
        Builtin {
            formulas: formulas.clone(),
            id: "quote",
            dependencies: &[],
            slot: Slot::Command("quote"),
        },
        Builtin {
            formulas: formulas.clone(),
            id: "paragraph-heading",
            dependencies: &[],
            slot: Slot::Command("text-block"),
        },
        Builtin {
            formulas: formulas.clone(),
            id: "inline",
            dependencies: &["code", "table"],
            slot: Slot::InputRule("complete"),
        },
        Builtin {
            formulas: formulas.clone(),
            id: "formula",
            dependencies: &["inline"],
            slot: Slot::Renderer("inline-math"),
        },
        Builtin {
            formulas: formulas.clone(),
            id: "highlight",
            dependencies: &["code"],
            slot: Slot::Decoration("syntax"),
        },
        Builtin {
            formulas: formulas.clone(),
            id: "block-ui",
            dependencies: &[],
            slot: Slot::Ui("block-controls"),
        },
        Builtin {
            formulas: formulas.clone(),
            id: "templates",
            dependencies: &["code", "list", "table"],
            slot: Slot::InsertTemplate("append"),
        },
    ];
    let mut features = builtins
        .iter()
        .map(|f| f as &dyn BuiltinFeature)
        .collect::<Vec<_>>();
    features.extend(previews.iter().map(|p| p as &dyn BuiltinFeature));
    features.extend(images.map(|p| p as &dyn BuiltinFeature));
    features.extend(additional.iter().map(|f| f.as_ref() as &dyn BuiltinFeature));
    Registry::build(&features)
}

/// Legacy control-only entry point participates in the same atomic registration.
pub(super) struct ControlAdapter(pub Arc<dyn RenderActionProvider>);
impl FeatureModule for ControlAdapter {
    fn manifest(&self) -> FeatureManifest {
        FeatureManifest::builtin(self.0.id())
    }
    fn register(&self, r: &mut Registrar) -> Result<(), String> {
        r.render_controls(
            Contribution {
                id: self.0.id(),
                slot: Slot::Ui("render-controls"),
                exclusive: false,
                priority: 0,
            },
            self.0.clone(),
        )
    }
}
