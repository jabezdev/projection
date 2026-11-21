use std::sync::{Arc, Mutex, OnceLock};

use cosmic_text::{
    Family, FontSystem,
};

use super::*;

impl Fonts {
    pub fn load() -> Arc<Self> {
        let loading = std::thread::Builder::new()
            .name("font-load".into())
            .spawn(|| {
                let mut system = FontSystem::new();
                set_generic_families(&mut system);
                system.db_mut().load_font_data(ICON_FONT.to_vec());
                system
            })
            .expect("spawning font loader");
        Arc::new(Self {
            system: OnceLock::new(),
            loading: Mutex::new(Some(loading)),
            substitutes: Mutex::default(),
            metrics: Mutex::default(),
            space_kerning: Mutex::default(),
        })
    }

    pub fn with<T>(&self, f: impl FnOnce(&mut FontSystem) -> T) -> T {
        let system = self.system.get_or_init(|| {
            let handle = self.loading.lock().expect("font loader poisoned").take();
            Mutex::new(handle.and_then(|h| h.join().ok()).unwrap_or_else(FontSystem::new))
        });
        f(&mut system.lock().expect("font system poisoned"))
    }

    /// The family to use for `requested`: itself when installed, else what
    /// the platform would substitute (fontconfig on Linux).
    pub(super) fn resolve(&self, system: &FontSystem, requested: &str) -> FamilyChoice {
        let key = requested.trim().to_ascii_lowercase();
        match key.as_str() {
            "" | "sans-serif" => return FamilyChoice::SansSerif,
            "serif" => return FamilyChoice::Serif,
            "monospace" => return FamilyChoice::Monospace,
            _ => {}
        }
        if let Some(choice) = self.substitutes.lock().expect("font substitutes poisoned").get(&key) {
            return choice.clone();
        }
        let choice = if installed(system, requested.trim()) {
            FamilyChoice::Named(requested.trim().to_string())
        } else {
            platform_substitute(requested.trim())
                .filter(|name| installed(system, name))
                .map(FamilyChoice::Named)
                .unwrap_or(FamilyChoice::SansSerif)
        };
        self.substitutes.lock().expect("font substitutes poisoned").insert(key, choice.clone());
        choice
    }

    /// Blink's line metrics for a font at `size`: rounded ascent, descent
    /// and line gap in pixels.
    pub(super) fn line_metrics(&self, system: &mut FontSystem, id: cosmic_text::fontdb::ID, weight: cosmic_text::fontdb::Weight, size: f32) -> Option<(f32, f32, f32)> {
        let cached = self.metrics.lock().expect("font metrics poisoned").get(&id).copied();
        let em = match cached {
            Some(em) => em,
            None => {
                let font = system.get_font(id, weight)?;
                let m = font.as_swash().metrics(&[]);
                let units = m.units_per_em.max(1) as f32;
                let em = (m.ascent / units, m.descent.abs() / units, m.leading / units);
                self.metrics.lock().expect("font metrics poisoned").insert(id, em);
                em
            }
        };
        Some(((em.0 * size).round(), (em.1 * size).round(), (em.2 * size).round()))
    }
}

pub(super) fn installed(system: &FontSystem, name: &str) -> bool {
    system.db().faces().any(|face| face.families.iter().any(|(n, _)| n.eq_ignore_ascii_case(name)))
}

/// Points the generic families at what browsers use on this platform.
pub(super) fn set_generic_families(system: &mut FontSystem) {
    #[cfg(target_os = "linux")]
    let generics = [
        ("sans-serif", fontconfig_match("sans-serif")),
        ("serif", fontconfig_match("serif")),
        ("monospace", fontconfig_match("monospace")),
    ];
    #[cfg(not(target_os = "linux"))]
    let generics = [
        ("sans-serif", Some("Arial".to_string())),
        ("serif", Some("Times New Roman".to_string())),
        ("monospace", Some("Consolas".to_string())),
    ];
    for (generic, family) in generics {
        let Some(family) = family.filter(|f| installed(system, f)) else { continue };
        let db = system.db_mut();
        match generic {
            "sans-serif" => db.set_sans_serif_family(family),
            "serif" => db.set_serif_family(family),
            _ => db.set_monospace_family(family),
        }
    }
}

/// Families substituted for missing ones when fontconfig is unavailable.
pub(super) const ALIASES: &[(&str, &str)] = &[
    ("arial", "Liberation Sans"),
    ("helvetica", "Liberation Sans"),
    ("times new roman", "Liberation Serif"),
    ("courier new", "Liberation Mono"),
];

pub(super) fn platform_substitute(requested: &str) -> Option<String> {
    #[cfg(target_os = "linux")]
    if let Some(family) = fontconfig_match(requested) {
        return Some(family);
    }
    let key = requested.to_ascii_lowercase();
    ALIASES.iter().find(|(name, _)| *name == key).map(|(_, alias)| alias.to_string())
}

#[cfg(target_os = "linux")]
pub(super) fn fontconfig_match(pattern: &str) -> Option<String> {
    let output = std::process::Command::new("fc-match").args(["-f", "%{family[0]}", pattern]).output().ok()?;
    let family = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (output.status.success() && !family.is_empty()).then_some(family)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum FamilyChoice {
    Named(String),
    SansSerif,
    Serif,
    Monospace,
}

impl FamilyChoice {
    pub(super) fn family(&self) -> Family<'_> {
        match self {
            FamilyChoice::Named(name) => Family::Name(name),
            FamilyChoice::SansSerif => Family::SansSerif,
            FamilyChoice::Serif => Family::Serif,
            FamilyChoice::Monospace => Family::Monospace,
        }
    }
}
