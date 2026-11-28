mod credits;
mod edit;
mod id;
mod media;
mod section;
mod service;
mod song;
mod template;

pub use credits::{Author, AuthorRole, Credits};
pub use edit::{diff_service, diff_song, section_label, Change, EditStep, ItemField, ServiceField, ServiceOp, SongField, SongOp};
pub use id::new_id;
pub use media::{MediaFile, MediaKind};
pub use section::split_slides;
pub use service::{Entry, ItemContent, Service, ServiceItem, SongItem};
pub use song::{Section, SectionKind, SectionPatch, Song, SongVersion};
pub use template::{ServiceTemplate, TemplatePart, DEFAULT_TEMPLATE_ID};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod template_tests;
