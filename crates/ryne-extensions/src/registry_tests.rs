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
