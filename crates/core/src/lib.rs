//! OpenDesigner core: the document model, validated operations, history and project files.
//!
//! This crate has no UI dependency. The Qt shell, the MCP server and the CLI are front ends over
//! the operations defined here (`docs/wiki/architecture.md`).

pub mod animation;
pub mod annotations;
pub mod arrange;
pub mod audio;
pub mod captions;
pub mod color;
pub mod device3d;
pub mod dryrun;
pub mod ease;
pub mod error;
pub mod geometry;
pub mod grade;
pub mod history;
mod index;
pub mod limits;
pub mod lint;
pub mod media;
pub mod model;
pub mod morph;
pub mod ops;
pub mod paint;
pub mod path;
pub mod pauses;
pub mod project;
pub mod scene;
mod snap;
pub mod speed;
pub mod timebase;
pub mod transform;
pub mod variables;
pub mod vector;
pub mod video;
pub mod view;
pub mod workspace;

pub use animation::{AnimProp, Animation, Key, Preset, PresetRequest, Repeat};
pub use annotations::{Annotation, AnnotationStatus, AnnotationTarget, Annotations};
pub use audio::{Duck, NewTrack, SoundTrack, TrackPatch, TrackRole};
pub use captions::{CaptionPosition, CaptionPreset, CaptionStyle, Captions, Cue, NewCue, Word};
pub use device3d::{Device3d, DeviceModel};
pub use dryrun::{OpFailure, OpReport, OpsCheck};
pub use ease::{Bezier, Ease, Ring, Spring};
pub use error::{OpError, ProjectError};
pub use grade::{Grade, GradeParam, Look};
pub use history::{Actor, Checkpoint, Session, Transaction, TransactionKind, TxReceipt};
pub use model::{Document, ImageFit, Node, NodeId, NodeKind, Page, Stroke, TextAlign};
pub use ops::{AppliedOp, NewNode, Operation, Property};
pub use paint::{Paint, Rgba, Stop};
pub use path::Path;
pub use speed::SpeedKey;
pub use transform::{Affine, Camera, Shadow};
pub use variables::{Binding, Portrays, VarValue, Variable, VariableKind};
pub use vector::{FillRule, LineCap, LineJoin, ViewBox};
pub use video::{
    AudioTrack, Clip, ClipProperty, ClipSource, Entrance, Footage, Motion, Overlay,
    OverlayProperty, Transition, TransitionKind, Video, VideoProperty,
};
pub use workspace::{Guidance, Workspace};
