// Animation system basics

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Easing {
    Linear,
    EaseInOutQuad,
    // Add more as needed
}

#[derive(Debug, Clone)]
pub struct Keyframe<T> {
    pub time: f64,
    pub value: T,
    pub easing: Easing,
}

pub trait Interpolatable {
    fn interpolate(&self, other: &Self, t: f64) -> Self;
}

impl Interpolatable for f32 {
    fn interpolate(&self, other: &Self, t: f64) -> Self {
        self + (other - self) * t as f32
    }
}

impl Interpolatable for glam::Vec3 {
    fn interpolate(&self, other: &Self, t: f64) -> Self {
        self.lerp(*other, t as f32)
    }
}

pub struct Track<T> {
    pub keyframes: Vec<Keyframe<T>>,
}

impl<T: Interpolatable + Clone> Track<T> {
    pub fn new() -> Self {
        Self { keyframes: Vec::new() }
    }

    pub fn get_value_at(&self, time: f64) -> Option<T> {
        if self.keyframes.is_empty() {
            return None;
        }
        
        if time <= self.keyframes.first().unwrap().time {
            return Some(self.keyframes.first().unwrap().value.clone());
        }
        if time >= self.keyframes.last().unwrap().time {
            return Some(self.keyframes.last().unwrap().value.clone());
        }

        for i in 0..self.keyframes.len() - 1 {
            let k1 = &self.keyframes[i];
            let k2 = &self.keyframes[i + 1];
            if time >= k1.time && time < k2.time {
                let duration = k2.time - k1.time;
                let t_raw = (time - k1.time) / duration;
                
                let t_eased = match k1.easing {
                    Easing::Linear => t_raw,
                    Easing::EaseInOutQuad => {
                        if t_raw < 0.5 { 2.0 * t_raw * t_raw } else { 1.0 - (-2.0 * t_raw + 2.0).powi(2) / 2.0 }
                    }
                };

                return Some(k1.value.interpolate(&k2.value, t_eased));
            }
        }
        None
    }
}
