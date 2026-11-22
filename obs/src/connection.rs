use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use protocol::{FeedState, FEED_PATH};
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::Message;

use crate::discover::{self, Found, Sharing};

const HOLD: Duration = Duration::from_secs(5);
const SILENCE: Duration = Duration::from_secs(40);
const CONNECT: Duration = Duration::from_secs(3);
const RETRY_FIRST: Duration = Duration::from_millis(500);
const RETRY_MAX: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Status {
    #[default]
    Searching,
    NotFound,
    Connected(String),
    Lost(String),
    Unreachable(String),
    SharingOff(String),
    PaltoTooOld(String),
    PluginTooOld(String),
}

impl Status {
    pub fn describe(&self) -> String {
        match self {
            Status::Searching => "Looking for PALTO on the network…".into(),
            Status::NotFound => "PALTO not found. Check that it is running on the same network, or type its address.".into(),
            Status::Connected(name) => format!("Connected to {name}."),
            Status::Lost(name) => format!("Lost the connection to {name}. Reconnecting…"),
            Status::Unreachable(name) => format!("Found {name}, but cannot connect. Check the firewall on the PALTO computer."),
            Status::SharingOff(name) => format!("Found {name}, but lyrics sharing is off. Turn on Settings → Remote → Share lyrics with OBS in PALTO."),
            Status::PaltoTooOld(name) => format!("{name} runs an older PALTO. Update PALTO."),
            Status::PluginTooOld(name) => format!("{name} runs a newer PALTO. Update this plugin."),
        }
    }

    pub fn is_problem(&self) -> bool {
        !matches!(self, Status::Connected(_) | Status::Searching)
    }
}

#[derive(Default)]
struct View {
    state: Option<FeedState>,
    status: Status,
    lost_at: Option<Instant>,
}

pub struct Connection {
    view: Arc<Mutex<View>>,
    _stop: watch::Sender<()>,
}

impl Connection {
    pub fn current(&self) -> Option<FeedState> {
        let view = self.view.lock().ok()?;
        let fresh = matches!(view.status, Status::Connected(_)) || view.lost_at.is_some_and(|at| at.elapsed() < HOLD);
        view.state.clone().filter(|_| fresh)
    }

    pub fn status(&self) -> Status {
        self.view.lock().map(|v| v.status.clone()).unwrap_or_default()
    }
}

static HUB: Mutex<Vec<(String, Weak<Connection>)>> = Mutex::new(Vec::new());

pub fn connect(target: &str) -> Arc<Connection> {
    let target = target.trim().to_string();
    let mut hub = HUB.lock().expect("hub poisoned");
    hub.retain(|(_, weak)| weak.strong_count() > 0);
    if let Some(existing) = hub.iter().find(|(t, _)| *t == target).and_then(|(_, weak)| weak.upgrade()) {
        return existing;
    }
    let view = Arc::new(Mutex::new(View::default()));
    let (stop, stopped) = watch::channel(());
    let connection = Arc::new(Connection { view: view.clone(), _stop: stop });
    hub.push((target.clone(), Arc::downgrade(&connection)));
    std::thread::Builder::new()
        .name("palto-feed".into())
        .spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
            runtime.block_on(async move {
                let mut stopped = stopped;
                tokio::select! {
                    _ = stopped.changed() => {}
                    _ = run(target, view) => {}
                }
            });
        })
        .ok();
    connection
}

fn set(view: &Mutex<View>, change: impl FnOnce(&mut View)) {
    if let Ok(mut view) = view.lock() {
        let before = view.status.clone();
        change(&mut view);
        if view.status != before {
            crate::data::log(crate::ffi::LOG_INFO, &view.status.describe());
        }
    }
}

async fn locate(target: &str) -> Option<Found> {
    if let Some(addr) = discover::parse_address(target) {
        return discover::check(addr).await;
    }
    let found = discover::scan().await;
    let matching = found.into_iter().filter(|f| target.is_empty() || f.name.eq_ignore_ascii_case(target));
    let mut matching: Vec<Found> = matching.collect();
    matching.sort_by_key(|f| f.sharing != Sharing::On);
    matching.into_iter().next()
}

async fn run(target: String, view: Arc<Mutex<View>>) {
    let mut retry = RETRY_FIRST;
    loop {
        match locate(&target).await {
            None => set(&view, |v| v.status = Status::NotFound),
            Some(found) => match found.sharing {
                Sharing::Off => set(&view, |v| v.status = Status::SharingOff(found.name.clone())),
                Sharing::PaltoTooOld => set(&view, |v| v.status = Status::PaltoTooOld(found.name.clone())),
                Sharing::PluginTooOld => set(&view, |v| v.status = Status::PluginTooOld(found.name.clone())),
                Sharing::On => {
                    if follow(&found, &view).await {
                        retry = RETRY_FIRST;
                    }
                    set(&view, |v| {
                        v.lost_at.get_or_insert_with(Instant::now);
                        v.status = match v.status {
                            Status::Connected(_) | Status::Lost(_) => Status::Lost(found.name.clone()),
                            _ => Status::Unreachable(found.name.clone()),
                        };
                    });
                }
            },
        }
        tokio::time::sleep(retry).await;
        retry = (retry * 2).min(RETRY_MAX);
    }
}

async fn follow(found: &Found, view: &Mutex<View>) -> bool {
    let url = format!("ws://{}{FEED_PATH}", found.addr);
    let Ok(Ok((socket, _))) = tokio::time::timeout(CONNECT, tokio_tungstenite::connect_async(url)).await else {
        return false;
    };
    let (_sink, mut source) = socket.split();
    let mut last_seq = 0;
    let mut connected = false;
    while let Ok(Some(Ok(message))) = tokio::time::timeout(SILENCE, source.next()).await {
        let Message::Text(text) = message else { continue };
        let Ok(state) = serde_json::from_str::<FeedState>(&text) else { continue };
        if state.seq <= last_seq {
            continue;
        }
        last_seq = state.seq;
        connected = true;
        set(view, |v| {
            v.state = Some(state);
            v.status = Status::Connected(found.name.clone());
            v.lost_at = None;
        });
    }
    connected
}
