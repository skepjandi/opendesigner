use thiserror::Error;

/// Why an operation was rejected. The text is shown to agents, so it names the rule that failed.
#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum OpError {
    #[error("node {0} does not exist")]
    NoSuchNode(String),
    #[error("node id {0} is already in use")]
    DuplicateId(String),
    #[error("invalid node id {0:?}: use 1-64 characters from A-Z a-z 0-9 _ -")]
    InvalidId(String),
    #[error("{what} is out of bounds: {detail}")]
    OutOfBounds { what: &'static str, detail: String },
    #[error("cannot move node {node} into its own descendant {target}")]
    Cycle { node: String, target: String },
    #[error("index {index} is out of range: parent {parent} has {len} children")]
    BadIndex {
        parent: String,
        index: usize,
        len: usize,
    },
    #[error("the root of page {0} cannot be moved, deleted or restyled")]
    RootImmutable(String),
    #[error("property {property} does not apply to {kind} nodes")]
    NotApplicable {
        property: &'static str,
        kind: &'static str,
    },
    #[error("a {parent_kind} node cannot have children")]
    NoChildren { parent_kind: &'static str },
    #[error("invalid paint {0:?}: use #rrggbb, #rrggbbaa or a {{token.name}} reference")]
    InvalidPaint(String),
    #[error("unknown design token {0}")]
    UnknownToken(String),
    #[error(
        "design token {name} is used by {count} node(s); change those first or set the token to another color instead"
    )]
    TokenInUse { name: String, count: usize },
    #[error("limit exceeded: {0}")]
    Limit(String),
    #[error("operation {0} can only be issued by the history, not by a front end")]
    InternalOnly(&'static str),
    #[error("nothing to {0}")]
    NothingTo(&'static str),
    #[error("checkpoint {0} does not exist")]
    NoSuchCheckpoint(u64),
    #[error("annotation {0} does not exist")]
    NoSuchAnnotation(u64),
    #[error("page {0} does not exist")]
    NoSuchPage(String),
    #[error("video {0} does not exist")]
    NoSuchVideo(String),
    #[error("clip {0} does not exist")]
    NoSuchClip(String),
    #[error(
        "frame {frame} is shown by clip {clip} of video {video}; remove that clip (or show another frame) first"
    )]
    FrameInUse {
        frame: String,
        clip: String,
        video: String,
    },
    #[error("cannot arrange: {0}")]
    Arrange(String),
    #[error("internal error: {0}")]
    Internal(String),
}

/// Why a project could not be read or written.
#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("{path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}: {message}")]
    Parse { path: String, message: String },
    #[error(
        "project format {found} is newer than this build supports ({supported}); update OpenDesigner"
    )]
    NewerFormat { found: u32, supported: u32 },
    #[error("project is invalid: {0}")]
    Invalid(String),
    #[error("{path} is {size} bytes, over the {limit} byte limit")]
    TooLarge { path: String, size: u64, limit: u64 },
}
