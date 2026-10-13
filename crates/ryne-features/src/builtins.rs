use ryne_extensions::*;
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
        Ok(())
    }
}
pub(crate) fn registry(previews: &[crate::diagrams::FencedPreview]) -> Result<Registry, String> {
    let builtins = [
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
    Registry::build(&features)
}
