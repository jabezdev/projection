use std::sync::{Arc, RwLock};

use crossbeam::channel::{unbounded, Receiver, Sender};
use protocol::{
    EngineSnapshot, OutputConfig,
};
use render_engine::{
    ConfidenceScene, MainScene,
    OutputRole, Scene,
};

use super::*;

#[derive(Clone)]
pub struct SceneFeed {
    pub(super) refresh: Sender<()>,
}

impl SceneFeed {
    pub fn start(
        snapshot: Arc<RwLock<EngineSnapshot>>,
        config: impl Fn() -> OutputConfig + Send + 'static,
        apply: impl Fn(Vec<(OutputRole, Scene)>, &MediaPlan) + Send + 'static,
    ) -> anyhow::Result<Self> {
        let (refresh, requests) = unbounded();
        std::thread::Builder::new().name("scene-feed".into()).spawn(move || run(&requests, &snapshot, config, apply))?;
        let feed = Self { refresh };
        feed.refresh();
        Ok(feed)
    }

    pub fn refresh(&self) {
        let _ = self.refresh.send(());
    }
}

pub(super) fn run(
    requests: &Receiver<()>,
    snapshot: &RwLock<EngineSnapshot>,
    config: impl Fn() -> OutputConfig,
    apply: impl Fn(Vec<(OutputRole, Scene)>, &MediaPlan),
) {
    let mut last: Option<(MainScene, ConfidenceScene, MediaPlan)> = None;
    while requests.recv().is_ok() {
        while requests.try_recv().is_ok() {}
        let config = config();
        let (main, confidence, plan) = {
            let state = snapshot.read().expect("snapshot poisoned");
            let (program, project, playback) = (&state.program_state, state.project.as_ref(), state.playback.as_ref());
            let main = main_scene(&config, program, project, playback);
            let plan = media_plan(&config, &main, program, project, playback);
            (main, confidence_scene(&config, program, project, playback), plan)
        };
        if last.as_ref().is_some_and(|(m, c, p)| *m == main && *c == confidence && *p == plan) {
            continue;
        }
        let mut scenes = Vec::new();
        if last.as_ref().is_none_or(|(m, ..)| *m != main) {
            scenes.push((OutputRole::Projection, Scene::Main(main.clone())));
        }
        if last.as_ref().is_none_or(|(_, c, _)| *c != confidence) {
            scenes.push((OutputRole::Confidence, Scene::Confidence(confidence.clone())));
        }
        apply(scenes, &plan);
        last = Some((main, confidence, plan));
    }
}
