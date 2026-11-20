//! OpenDesigner core: the document model, validated operations, history and project files.
//!
//! This crate has no UI dependency. The Qt shell, the MCP server and the CLI are front ends over
//! the operations defined here (`docs/wiki/architecture.md`).

pub mod animation;
pub mod annotations;
pub mod arrange;
pub mod device3d;
pub mod dryrun;
pub mod error;
pub mod geometry;
pub mod history;
pub mod limits;
pub mod lint;
pub mod media;
pub mod model;
pub mod ops;
pub mod paint;
pub mod project;
pub mod variables;
pub mod video;
pub mod view;
pub mod workspace;

pub use animation::{AnimProp, Animation, Key, Preset, PresetRequest};
pub use annotations::{Annotation, AnnotationStatus, AnnotationTarget, Annotations};
pub use device3d::{Device3d, DeviceModel};
pub use dryrun::{OpFailure, OpReport, OpsCheck};
pub use error::{OpError, ProjectError};
pub use history::{Actor, Checkpoint, Session, Transaction, TransactionKind, TxReceipt};
pub use model::{Document, ImageFit, Node, NodeId, NodeKind, Page, Stroke, TextAlign};
pub use ops::{AppliedOp, NewNode, Operation, Property};
pub use paint::{Paint, Rgba};
pub use variables::{Binding, Portrays, VarValue, Variable, VariableKind};
pub use video::{
    AudioTrack, Clip, ClipProperty, ClipSource, Ease, Entrance, Footage, Motion, Overlay,
    OverlayProperty, Transition, TransitionKind, Video, VideoProperty,
};
pub use workspace::{Guidance, Workspace};
