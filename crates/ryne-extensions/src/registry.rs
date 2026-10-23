//! Startup-only registration. Contributions are staged before the registry is published.
use crate::{
    CommandHandler, CommandPlan, EditIntent, EditingState, FeatureServices, InsertBlock,
    PresentationProvider, PreviewInput, RenderActionProvider, RenderActionRegistry,
    SyntaxHighlighter,
};
use ryne_projection::{DiagramViewInput, Display};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
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
    commands: BTreeMap<&'static str, Arc<dyn CommandHandler>>,
    presentations: BTreeMap<&'static str, Arc<dyn PresentationProvider>>,
    controls: BTreeMap<&'static str, Arc<dyn RenderActionProvider>>,
    previews: BTreeMap<&'static str, Arc<dyn PreviewProvider>>,
    block_ui: BTreeMap<&'static str, Arc<dyn BlockUiProvider>>,
    templates: BTreeMap<&'static str, Vec<(&'static str, InsertBlock)>>,
}
impl Registrar {
    pub fn contribute(&mut self, contribution: Contribution) {
        self.staged.push(contribution)
    }
    pub fn render_controls(
        &mut self,
        contribution: Contribution,
        provider: Arc<dyn RenderActionProvider>,
    ) -> Result<(), String> {
        if !matches!(contribution.slot, Slot::Ui(_)) {
            return Err("incompatible contribution slot".into());
        }
        if self.controls.insert(contribution.id, provider).is_some() {
            return Err("duplicate executable contribution".into());
        }
        self.contribute(contribution);
        Ok(())
    }
    pub fn preview(
        &mut self,
        contribution: Contribution,
        provider: Arc<dyn PreviewProvider>,
    ) -> Result<(), String> {
        if !matches!(contribution.slot, Slot::Renderer(_)) {
            return Err("incompatible contribution slot".into());
        }
        if self.previews.insert(contribution.id, provider).is_some() {
            return Err("duplicate executable contribution".into());
        }
        self.contribute(contribution);
        Ok(())
    }
    pub fn block_ui(
        &mut self,
        contribution: Contribution,
        provider: Arc<dyn BlockUiProvider>,
    ) -> Result<(), String> {
        if contribution.slot != Slot::Ui("block-menu") || !contribution.exclusive {
            return Err("incompatible contribution slot".into());
        }
        if self.block_ui.insert(contribution.id, provider).is_some() {
            return Err("duplicate executable contribution".into());
        }
        self.contribute(contribution);
        Ok(())
    }
    pub fn templates(
        &mut self,
        contribution: Contribution,
        provider: Vec<(&'static str, InsertBlock)>,
    ) -> Result<(), String> {
        if !matches!(contribution.slot, Slot::InsertTemplate(_)) {
            return Err("incompatible contribution slot".into());
        }
        if self.templates.insert(contribution.id, provider).is_some() {
            return Err("duplicate executable contribution".into());
        }
        self.contribute(contribution);
        Ok(())
    }
    pub fn command(
        &mut self,
        contribution: Contribution,
        handler: Arc<dyn CommandHandler>,
    ) -> Result<(), String> {
        if !matches!(contribution.slot, Slot::Command(_) | Slot::InputRule(_)) {
            return Err("command handler requires a command or input slot".into());
        }
        if self.commands.insert(contribution.id, handler).is_some() {
            return Err("duplicate executable contribution".into());
        }
        self.contribute(contribution);
        Ok(())
    }
    pub fn presentation(
        &mut self,
        contribution: Contribution,
        provider: Arc<dyn PresentationProvider>,
    ) -> Result<(), String> {
        if contribution.slot != Slot::PrimaryPresentation("document") || !contribution.exclusive {
            return Err("document presentation must be exclusive".into());
        }
        if self
            .presentations
            .insert(contribution.id, provider)
            .is_some()
        {
            return Err("duplicate executable contribution".into());
        }
        self.contribute(contribution);
        Ok(())
    }
}
pub trait FeatureModule {
    fn manifest(&self) -> FeatureManifest;
    fn register(&self, registrar: &mut Registrar) -> Result<(), String>;
}
/// Compatibility name for existing static feature implementations.
pub use FeatureModule as BuiltinFeature;
pub trait PreviewProvider: Send + Sync {
    fn input(
        &self,
        state: &EditingState,
        block: &ryne_syntax::code_block::CodeBlock,
    ) -> Option<PreviewInput>;
}
pub trait BlockUiProvider: Send + Sync {
    fn adornment(&self, block: &ryne_syntax::block::Block, source_open: bool) -> AdornmentSpec;
}
#[derive(Clone, Default)]
pub struct Registry {
    render_actions: RenderActionRegistry,
    insert_templates: Vec<(&'static str, InsertBlock)>,
    order: Vec<&'static str>,
    contributions: Vec<(&'static str, Contribution)>,
    commands: BTreeMap<&'static str, Arc<dyn CommandHandler>>,
    presentations: BTreeMap<&'static str, Arc<dyn PresentationProvider>>,
    previews: BTreeMap<&'static str, Arc<dyn PreviewProvider>>,
    block_ui: BTreeMap<&'static str, Arc<dyn BlockUiProvider>>,
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
        let mut commands = BTreeMap::new();
        let mut presentations = BTreeMap::new();
        let mut controls = BTreeMap::new();
        let mut previews = BTreeMap::new();
        let mut block_ui = BTreeMap::new();
        let mut templates = BTreeMap::new();
        let mut ids = BTreeSet::new();
        let mut slots = BTreeMap::new();
        for id in &order {
            let mut registrar = Registrar::default();
            features[manifests[id].0].register(&mut registrar)?;
            commands.extend(registrar.commands);
            presentations.extend(registrar.presentations);
            controls.extend(registrar.controls);
            previews.extend(registrar.previews);
            block_ui.extend(registrar.block_ui);
            templates.extend(registrar.templates);
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
        let render_actions = RenderActionRegistry::new(
            contributions
                .iter()
                .filter_map(|(_, c)| controls.get(c.id).cloned())
                .collect(),
        )?;
        let insert_templates = contributions
            .iter()
            .filter_map(|(_, c)| templates.get(c.id))
            .flatten()
            .cloned()
            .collect();
        Ok(Self {
            render_actions,
            insert_templates,
            previews,
            block_ui,
            order,
            contributions,
            commands,
            presentations,
        })
    }
    pub fn plan(
        &self,
        intent: &EditIntent,
        state: &EditingState,
        services: &FeatureServices<'_>,
    ) -> Result<Option<CommandPlan>, String> {
        for (_, contribution) in &self.contributions {
            if let Some(handler) = self.commands.get(contribution.id) {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    handler.plan(intent, state, services)
                }))
                .map_err(|_| format!("{}: command handler panicked", contribution.id))??;
                if result.is_some() {
                    return Ok(result);
                }
            }
        }
        Ok(None)
    }
    pub fn display(
        &self,
        state: &EditingState,
        services: &FeatureServices<'_>,
        highlighter: Option<&mut dyn SyntaxHighlighter>,
        diagrams: &[DiagramViewInput],
    ) -> Option<Display> {
        self.contributions
            .iter()
            .find_map(|(_, contribution)| self.presentations.get(contribution.id))
            .map(|provider| provider.display(state, services, highlighter, diagrams))
    }
    pub fn render_actions(&self) -> &RenderActionRegistry {
        &self.render_actions
    }
    pub fn insert_templates(&self) -> &[(&'static str, InsertBlock)] {
        &self.insert_templates
    }
    pub fn preview_inputs(&self, state: &EditingState) -> Vec<PreviewInput> {
        ryne_syntax::SemanticQuery::new(&state.parse)
            .code_blocks()
            .iter()
            .filter_map(|block| {
                self.contributions
                    .iter()
                    .find_map(|(_, c)| self.previews.get(c.id).and_then(|p| p.input(state, block)))
            })
            .take(32)
            .collect()
    }
    pub fn block_ui(&self, block: &ryne_syntax::block::Block, source_open: bool) -> AdornmentSpec {
        self.contributions
            .iter()
            .find_map(|(_, c)| self.block_ui.get(c.id))
            .map(|provider| provider.adornment(block, source_open))
            .unwrap_or_else(|| AdornmentSpec {
                metadata: BlockMetadata::from(block.kind),
                menu: Vec::new(),
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
    pub kind: Option<ryne_syntax::block::BlockKind>,
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
    AddBelow,
    AddAbove,
}
