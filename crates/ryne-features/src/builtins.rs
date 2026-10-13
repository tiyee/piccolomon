use crate::commands::dispatch::BuiltinCommand;
use ryne_extensions::*;
use std::sync::Arc;
pub(crate) struct Builtin {
    pub id: &'static str,
    pub dependencies: &'static [&'static str],
    pub slot: Slot,
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
                    priority: 0,
                },
                Arc::new(handler),
            )?;
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
    previews: &[crate::diagrams::FencedPreview],
    additional: &[Arc<dyn BuiltinFeature + Send + Sync>],
) -> Result<Registry, String> {
    let builtins = [
        Builtin {
            id: "format",
            dependencies: &[],
            slot: Slot::Command("format"),
        },
        Builtin {
            id: "document-presentation",
            dependencies: &["code", "table", "list", "formula"],
            slot: Slot::Ui("document-presentation"),
        },
        Builtin {
            id: "code",
            dependencies: &[],
            slot: Slot::PrimaryPresentation("code"),
        },
        Builtin {
            id: "list",
            dependencies: &[],
            slot: Slot::PrimaryPresentation("list"),
        },
        Builtin {
            id: "table",
            dependencies: &[],
            slot: Slot::PrimaryPresentation("table"),
        },
        Builtin {
            id: "paragraph-heading",
            dependencies: &[],
            slot: Slot::Command("text-block"),
        },
        Builtin {
            id: "inline",
            dependencies: &["code", "table"],
            slot: Slot::InputRule("complete"),
        },
        Builtin {
            id: "formula",
            dependencies: &["inline"],
            slot: Slot::Renderer("inline-math"),
        },
        Builtin {
            id: "highlight",
            dependencies: &["code"],
            slot: Slot::Decoration("syntax"),
        },
        Builtin {
            id: "block-ui",
            dependencies: &[],
            slot: Slot::Ui("block-controls"),
        },
        Builtin {
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
    features.extend(additional.iter().map(|f| f.as_ref() as &dyn BuiltinFeature));
    Registry::build(&features)
}
