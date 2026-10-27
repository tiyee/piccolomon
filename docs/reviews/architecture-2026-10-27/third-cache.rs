use ryne_extensions::*;
use ryne_session::{EditorSession, SessionServices};
use std::sync::Arc;
struct Commands;
impl FeatureModule for Commands {
    fn manifest(&self) -> FeatureManifest { FeatureManifest::builtin("extra-command") }
    fn register(&self, r: &mut Registrar) -> Result<(), String> {
        r.command(Contribution { id: "extra.command", slot: Slot::Command("edit"), exclusive: false, priority: 10 }, Arc::new(Self))
    }
}
impl CommandHandler for Commands {
    fn plan(&self, _: &EditIntent, _: &EditingState, _: &FeatureServices<'_>) -> Result<Option<CommandPlan>, String> { Ok(None) }
}
fn main() {
    let features = ryne_features::MarkdownFeatures::new(Arc::new(ryne_services::math::FormularyRenderer::default()),vec![]).unwrap().with_feature(Arc::new(Commands)).unwrap();
    let mut doc=EditorSession::new("plain 中😀 tail\n\n- first\n\n| a | b |\n| --- | --- |\n| x | y |\n",SessionServices { features: Arc::new(features),files: Arc::new(ryne_services::files::AtomicFileStore) });
    let display=doc.cached_project_batch(&doc,None,&[]).display;
    let controls=doc.cached_render_control_batch(&doc).unwrap().controls;
    let mut projections=0; let mut batches=0;
    for _ in 0..10 {
        doc.select(6,false,false);
        projections+=usize::from(Arc::ptr_eq(&display,&doc.cached_project_batch(&doc,None,&[]).display));
        batches+=usize::from(Arc::ptr_eq(&controls,&doc.cached_render_control_batch(&doc).unwrap().controls));
    }
    println!("command_only caret_refreshes=10 projection_reused={projections} controls_reused={batches} dirty={} undo={}",doc.dirty(),doc.history_len(false));
}
