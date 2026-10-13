//! Startup-only registration. Contributions are staged before the registry is published.
use std::collections::{BTreeMap, BTreeSet};
pub const INTERNAL_API: u32 = 1;
#[derive(Clone, Debug)]
pub struct FeatureManifest {
    pub id: &'static str,
    pub version: u32,
    pub api: u32,
    pub requires: Vec<&'static str>,
    pub before: Vec<&'static str>,
    pub after: Vec<&'static str>,
}
impl FeatureManifest {
    pub fn builtin(id: &'static str) -> Self {
        Self {
            id,
            version: 1,
            api: INTERNAL_API,
            requires: Vec::new(),
            before: Vec::new(),
            after: Vec::new(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Slot {
    Command(&'static str),
    InputRule(&'static str),
    PrimaryPresentation(&'static str),
    Decoration(&'static str),
    Ui(&'static str),
    InsertTemplate(&'static str),
    Renderer(&'static str),
}
#[derive(Clone, Debug)]
pub struct Contribution {
    pub id: &'static str,
    pub slot: Slot,
    pub exclusive: bool,
    pub priority: i32,
}
#[derive(Default)]
pub struct Registrar {
    staged: Vec<Contribution>,
}
impl Registrar {
    pub fn contribute(&mut self, contribution: Contribution) {
        self.staged.push(contribution)
    }
}
pub trait BuiltinFeature {
    fn manifest(&self) -> FeatureManifest;
    fn register(&self, registrar: &mut Registrar) -> Result<(), String>;
}
#[derive(Default)]
pub struct Registry {
    order: Vec<&'static str>,
    contributions: Vec<(&'static str, Contribution)>,
}
impl Registry {
    pub fn build(features: &[&dyn BuiltinFeature]) -> Result<Self, String> {
        let mut manifests = BTreeMap::new();
        for (index, feature) in features.iter().enumerate() {
            let manifest = feature.manifest();
            if manifest.api != INTERNAL_API {
                return Err(format!("incompatible API: {}", manifest.id));
            }
            let id = manifest.id;
            if manifests.insert(id, (index, manifest)).is_some() {
                return Err(format!("duplicate feature: {id}"));
            }
        }
        let mut edges: BTreeMap<&str, BTreeSet<&str>> =
            manifests.keys().map(|id| (*id, BTreeSet::new())).collect();
        for (&id, (_, manifest)) in &manifests {
            for required in &manifest.requires {
                if !manifests.contains_key(required) {
                    return Err(format!("{id} requires missing {required}"));
                }
                edges
                    .get_mut(required)
                    .ok_or("missing dependency")?
                    .insert(id);
            }
            for before in &manifest.before {
                if !manifests.contains_key(before) {
                    return Err(format!("missing order target: {before}"));
                }
                edges.get_mut(id).ok_or("missing feature")?.insert(before);
            }
            for after in &manifest.after {
                if !manifests.contains_key(after) {
                    return Err(format!("missing order target: {after}"));
                }
                edges
                    .get_mut(after)
                    .ok_or("missing order target")?
                    .insert(id);
            }
        }
        let mut order = Vec::new();
        let mut remaining: BTreeSet<_> = manifests.keys().copied().collect();
        while !remaining.is_empty() {
            let ready = remaining
                .iter()
                .copied()
                .find(|id| !remaining.iter().any(|other| edges[other].contains(id)));
            let Some(id) = ready else {
                return Err("feature dependency/order cycle".into());
            };
            remaining.remove(id);
            order.push(id);
        }
        let mut contributions = Vec::new();
        let mut ids = BTreeSet::new();
        let mut slots = BTreeMap::new();
        for id in &order {
            let mut registrar = Registrar::default();
            features[manifests[id].0].register(&mut registrar)?;
            // Nothing is visible to the host until all staged contributions validate.
            for contribution in registrar.staged {
                if !ids.insert(contribution.id) {
                    return Err(format!("duplicate contribution: {}", contribution.id));
                }
                if let Some(exclusive) = slots.get(&contribution.slot)
                    && (*exclusive || contribution.exclusive)
                {
                    return Err(format!("exclusive slot conflict: {:?}", contribution.slot));
                }
                slots.insert(contribution.slot.clone(), contribution.exclusive);
                contributions.push((*id, contribution));
            }
        }
        // Within a slot, explicit dependency/order constraints win; otherwise
        // priority wins before the stable ID. A feature's alphabetical topological
        // rank must not accidentally override an unrelated provider's priority.
        let reaches = |from: &str, to: &str| {
            let mut pending = vec![from];
            let mut visited = BTreeSet::new();
            while let Some(id) = pending.pop() {
                if visited.insert(id) {
                    for next in &edges[id] {
                        if *next == to {
                            return true;
                        }
                        pending.push(next);
                    }
                }
            }
            false
        };
        let mut sorted = Vec::new();
        while !contributions.is_empty() {
            let slot = contributions
                .iter()
                .map(|(_, c)| &c.slot)
                .min()
                .ok_or("missing contribution")?
                .clone();
            let index = contributions
                .iter()
                .enumerate()
                .filter(|(_, (feature, c))| {
                    c.slot == slot
                        && !contributions.iter().any(|(other, previous)| {
                            previous.slot == slot && reaches(other, feature)
                        })
                })
                .min_by(|(_, (fa, a)), (_, (fb, b))| {
                    b.priority
                        .cmp(&a.priority)
                        .then(a.id.cmp(b.id))
                        .then(fa.cmp(fb))
                })
                .map(|(i, _)| i)
                .ok_or("contribution order cycle")?;
            sorted.push(contributions.remove(index));
        }
        let contributions = sorted;
        Ok(Self {
            order,
            contributions,
        })
    }
    pub fn feature_order(&self) -> &[&'static str] {
        &self.order
    }
    pub fn contributions(&self) -> &[(&'static str, Contribution)] {
        &self.contributions
    }
}
#[derive(Clone, Debug)]
pub struct BlockMetadata {
    pub name: &'static str,
    pub icon: Option<&'static str>,
    pub label: &'static str,
}
#[derive(Clone, Debug)]
pub struct MenuItemSpec {
    pub command: BlockCommand,
    pub name: &'static str,
    pub order: u16,
}
#[derive(Clone, Debug)]
pub struct AdornmentSpec {
    pub metadata: BlockMetadata,
    pub menu: Vec<MenuItemSpec>,
}

#[derive(Clone, Copy, Debug)]
pub enum BlockCommand {
    Source,
    Select,
    Copy,
}
