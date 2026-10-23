//! Experimental contracts. No GPUI, concrete Markdown type or mutable document
//! enters RenderExtension. Fixture geometry is NOT a production layout engine.
use ryne_core::{DocumentKey, SourceRange};
use ryne_extensions::{CommandPlan, EditingState};
use ryne_session::EditorSession;
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub document: DocumentKey,
    pub role: &'static str,
    pub owner: Range<usize>,
    pub part: Vec<usize>,
}
impl Target {
    pub fn check(&self, state: &EditingState) -> Result<(), String> {
        SourceRange::new(self.document, self.owner.clone())
            .validate(&state.source)
            .map_err(|e| format!("{e:?}"))?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
impl Rect {
    pub fn valid(self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|n| n.is_finite())
            && self.width > 0.
            && self.height > 0.
    }
    pub fn contains(self, (x, y): (f32, f32)) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
    pub fn center(self) -> (f32, f32) {
        (self.x + self.width / 2., self.y + self.height / 2.)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Context {
    pub document: DocumentKey,
    // Production: projection + layout + viewport generations, including scroll.
    pub epoch: u64,
    pub source_mode: bool,
    pub composing: bool,
    pub dragging: bool,
}
impl Context {
    pub fn rendered(document: DocumentKey) -> Self {
        Self {
            document,
            epoch: 1,
            source_mode: false,
            composing: false,
            dragging: false,
        }
    }
    fn check(self, session: &EditorSession) -> Result<(), String> {
        if self.document != session.source.id()
            || self.source_mode
            || session.source_mode
            || self.composing
            || session.composition().is_some()
            || self.dragging
        {
            return Err("interaction unavailable or stale".into());
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug)]
pub enum Placement {
    // In production, trailing controls reserve this space before text shaping.
    Trailing,
    After,
}
#[derive(Clone, Debug)]
pub struct Action {
    pub command: &'static str,
    pub label_key: &'static str,
    pub enabled: bool,
    pub disabled_reason: Option<&'static str>,
}
#[derive(Clone, Debug)]
pub struct Contribution {
    pub provider: &'static str,
    pub id: String,
    pub target: Target,
    pub placement: Placement,
    pub priority: i32,
    pub actions: Vec<Action>,
    // One action is a direct button; multiple actions are a popup trigger.
}
pub struct Surface {
    pub target: Target,
    pub bounds: Rect,
}
#[derive(Clone, Debug)]
pub struct Control {
    pub spec: Contribution,
    pub bounds: Rect,
}
pub struct Frame {
    pub context: Context,
    pub controls: Vec<Control>,
}

pub trait RenderExtension {
    fn id(&self) -> &'static str;
    fn commands(&self) -> &'static [&'static str];
    fn contribute(&self, state: &EditingState) -> Result<Vec<Contribution>, String>;
    // Both eligibility and semantic resolution MUST run again at execution.
    fn available(&self, state: &EditingState, target: &Target, command: &str) -> bool;
    fn plan(
        &self,
        state: &EditingState,
        target: &Target,
        command: &str,
    ) -> Result<CommandPlan, String>;
}

pub struct Invocation {
    context: Context,
    provider: &'static str,
    target: Target,
    action: Action,
}
pub struct Menu {
    context: Context,
    spec: Contribution,
    open: bool,
}
pub enum Activation {
    Menu(Menu),
    Command(Invocation),
}
impl Menu {
    pub fn take(&mut self, index: usize) -> Result<Invocation, String> {
        if !self.open {
            return Err("menu closed".into());
        }
        let action = self
            .spec
            .actions
            .get(index)
            .ok_or("missing action")?
            .clone();
        if !action.enabled {
            return Err(action.disabled_reason.unwrap_or("disabled").into());
        }
        self.open = false;
        Ok(Invocation {
            context: self.context,
            provider: self.spec.provider,
            target: self.spec.target.clone(),
            action,
        })
    }
}
pub struct Host {
    providers: BTreeMap<&'static str, Box<dyn RenderExtension>>,
    commands: BTreeMap<&'static str, &'static str>,
}
impl Host {
    pub fn new(extensions: Vec<Box<dyn RenderExtension>>) -> Result<Self, String> {
        let mut providers = BTreeMap::new();
        let mut commands = BTreeMap::new();
        for extension in extensions {
            let id = extension.id();
            if providers.contains_key(id) {
                return Err("duplicate provider".into());
            }
            for &command in extension.commands() {
                if !command.starts_with(&format!("{id}.")) || commands.insert(command, id).is_some()
                {
                    return Err("invalid or duplicate command".into());
                }
            }
            providers.insert(id, extension);
        }
        Ok(Self {
            providers,
            commands,
        })
    }
    pub fn contribute(&self, state: &EditingState) -> Result<Vec<Contribution>, String> {
        if state.parse.source().id() != state.source.id() {
            return Err("stale semantics".into());
        }
        let mut result = Vec::new();
        let mut ids = BTreeSet::new();
        for (&id, provider) in &self.providers {
            for spec in provider.contribute(state)? {
                spec.target.check(state)?;
                if spec.provider != id
                    || !ids.insert((id, spec.id.clone()))
                    || spec.actions.is_empty()
                    || spec.actions.iter().any(|a| {
                        self.commands.get(a.command) != Some(&id)
                            || a.label_key.is_empty()
                            || (!a.enabled && a.disabled_reason.is_none())
                    })
                {
                    return Err("invalid contribution".into());
                }
                result.push(spec);
            }
        }
        result.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then(a.provider.cmp(b.provider))
                .then(a.id.cmp(&b.id))
        });
        Ok(result)
    }
    pub fn layout(
        &self,
        document: &EditorSession,
        context: Context,
        surfaces: &[Surface],
        contributions: Vec<Contribution>,
    ) -> Result<Frame, String> {
        context.check(document)?;
        let mut controls: Vec<Control> = Vec::new();
        for spec in contributions {
            spec.target.check(document)?;
            if document.block_source.as_ref().is_some_and(|s| {
                s.range.start < spec.target.owner.end && spec.target.owner.start < s.range.end
            }) {
                continue;
            }
            let surface = surfaces
                .iter()
                .find(|s| s.target == spec.target)
                .ok_or("missing surface")?;
            if !surface.bounds.valid() {
                return Err("invalid geometry".into());
            }
            let bounds = match spec.placement {
                Placement::Trailing => Rect {
                    x: surface.bounds.x + surface.bounds.width - 20.,
                    y: surface.bounds.y,
                    width: 20.,
                    height: 20.,
                },
                Placement::After => Rect {
                    x: surface.bounds.x,
                    y: surface.bounds.y + surface.bounds.height,
                    width: 20.,
                    height: 20.,
                },
            };
            // Production will group controls; the prototype reports conflicts
            // rather than silently hiding another extension's command.
            if controls.iter().any(|c| overlaps(c.bounds, bounds)) {
                return Err("overlapping controls require an overflow group".into());
            }
            controls.push(Control { spec, bounds });
        }
        Ok(Frame { context, controls })
    }
    pub fn activate(
        &self,
        document: &EditorSession,
        context: Context,
        frame: &Frame,
        point: (f32, f32),
    ) -> Result<Activation, String> {
        context.check(document)?;
        if context != frame.context {
            return Err("stale frame".into());
        }
        let control = frame
            .controls
            .iter()
            .find(|c| c.bounds.contains(point))
            .ok_or("no control")?;
        let mut menu = Menu {
            context,
            spec: control.spec.clone(),
            open: true,
        };
        if menu.spec.actions.len() == 1 {
            Ok(Activation::Command(menu.take(0)?))
        } else {
            Ok(Activation::Menu(menu))
        }
    }
    pub fn execute(
        &self,
        document: &mut EditorSession,
        context: Context,
        invocation: Invocation,
    ) -> Result<(), String> {
        context.check(document)?;
        if context != invocation.context {
            return Err("stale invocation".into());
        }
        invocation.target.check(document)?;
        if document.block_source.as_ref().is_some_and(|s| {
            s.range.start < invocation.target.owner.end
                && invocation.target.owner.start < s.range.end
        }) {
            return Err("target is in local source mode".into());
        }
        if self.commands.get(invocation.action.command) != Some(&invocation.provider) {
            return Err("unregistered command".into());
        }
        let provider = self
            .providers
            .get(invocation.provider)
            .ok_or("missing provider")?;
        if !invocation.action.enabled
            || !provider.available(document, &invocation.target, invocation.action.command)
        {
            return Err("command unavailable".into());
        }
        let plan = provider.plan(document, &invocation.target, invocation.action.command)?;
        document.publish_plan(plan)?;
        Ok(())
    }
}
fn overlaps(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}

/// Deterministic mock semantic surfaces; no claim about GPUI text measurements.
pub fn fixture_surfaces(specs: &[Contribution]) -> Vec<Surface> {
    specs
        .iter()
        .enumerate()
        .map(|(i, s)| Surface {
            target: s.target.clone(),
            bounds: Rect {
                x: 0.,
                y: i as f32 * 60.,
                width: 180.,
                height: 30.,
            },
        })
        .collect()
}
