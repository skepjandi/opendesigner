//! The arguments of a stagger as an agent writes them, read into a [`StaggerSpec`] and run over a
//! list of nodes: a delay for each, and, for a `range`, a value for each
//! (`docs/wiki/video-editing.md#stagger`).

use od_core::{Document, Ease, NodeId};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::scene_motion::{
    Axis, Metric, Origin, StaggerSpec, Step, stagger_offsets_ms, stagger_values,
};
use crate::tools::ToolError;

use super::{MAX_STAGGERED, extent_of, origin_of};

const MAX_EACH_MS: f64 = 5_000.0;

/// Where the wave starts: a word, the index of a node, or a point across the group.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum FromArg {
    Index(usize),
    Name(String),
    Point { point: [f64; 2] },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum JitterArg {
    Same(f64),
    Span([f64; 2]),
}

/// A stagger: every field optional, so a call says only what differs from a plain row of nodes
/// one step apart.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaggerArgs {
    pub each_ms: Option<f64>,
    pub range: Option<[f64; 2]>,
    pub from: Option<FromArg>,
    pub grid: Option<[usize; 2]>,
    pub columns: Option<usize>,
    pub metric: Option<String>,
    pub axis: Option<String>,
    pub ease: Option<Ease>,
    pub reversed: Option<bool>,
    pub start_ms: Option<f64>,
    pub jitter_ms: Option<JitterArg>,
    pub seed: Option<u64>,
    pub spatial: Option<bool>,
}

/// What a stagger gives each node.
pub struct Distribution {
    pub offsets_ms: Vec<u64>,
    /// With a `range`: the value of each node, which multiplies the move's intensity.
    pub values: Option<Vec<f64>>,
}

impl StaggerArgs {
    /// The fields of `self`, and for those it leaves out, the ones of `base`.
    pub fn over(self, base: StaggerArgs) -> StaggerArgs {
        StaggerArgs {
            each_ms: self.each_ms.or(base.each_ms),
            range: self.range.or(base.range),
            from: self.from.or(base.from),
            grid: self.grid.or(base.grid),
            columns: self.columns.or(base.columns),
            metric: self.metric.or(base.metric),
            axis: self.axis.or(base.axis),
            ease: self.ease.or(base.ease),
            reversed: self.reversed.or(base.reversed),
            start_ms: self.start_ms.or(base.start_ms),
            jitter_ms: self.jitter_ms.or(base.jitter_ms),
            seed: self.seed.or(base.seed),
            spatial: self.spatial.or(base.spatial),
        }
    }

    fn origin(&self, count: usize) -> Result<Origin, ToolError> {
        match &self.from {
            None => Ok(Origin::First),
            Some(FromArg::Name(name)) => Origin::parse(name).ok_or_else(|| {
                ToolError(format!(
                    "from {name:?} is not first, last, center, edges or random"
                ))
            }),
            Some(FromArg::Index(i)) if *i < count => Ok(Origin::Index(*i)),
            Some(FromArg::Index(_)) => Err(ToolError(format!(
                "from is not an index into the {count} nodes"
            ))),
            Some(FromArg::Point { point: [x, y] })
                if (0.0..=1.0).contains(x) && (0.0..=1.0).contains(y) =>
            {
                Ok(Origin::Point(*x, *y))
            }
            Some(FromArg::Point { .. }) => Err(ToolError(
                "from point is two fractions from 0 to 1 across the group".into(),
            )),
        }
    }

    fn grid(&self, count: usize) -> Result<Option<(usize, usize)>, ToolError> {
        match (self.grid, self.columns) {
            (Some(_), Some(_)) => Err(ToolError("give grid or columns, not both".into())),
            (Some([columns, rows]), None) => {
                if columns == 0 || rows == 0 || columns * rows < count {
                    return Err(ToolError(format!(
                        "grid {columns}x{rows} does not hold the {count} nodes"
                    )));
                }
                Ok(Some((columns, rows)))
            }
            (None, Some(columns)) => {
                if columns == 0 || columns > count {
                    return Err(ToolError(format!(
                        "columns {columns} is not from 1 to the {count} nodes"
                    )));
                }
                Ok(Some((columns, count.div_ceil(columns))))
            }
            (None, None) => Ok(None),
        }
    }

    fn jitter(&self) -> Result<Option<(f64, f64)>, ToolError> {
        let pair = match self.jitter_ms {
            None => return Ok(None),
            Some(JitterArg::Same(j)) => (j, j),
            Some(JitterArg::Span([a, b])) => (a, b),
        };
        let fine = |v: f64| v.is_finite() && (0.0..=MAX_EACH_MS).contains(&v);
        if fine(pair.0) && fine(pair.1) {
            Ok(Some(pair))
        } else {
            Err(ToolError(format!(
                "jitter_ms is from 0 to {MAX_EACH_MS} (one number, or the size at the first and at the last node)"
            )))
        }
    }

    /// The distribution over `ids`. `seed` is the move's seed, used for a random order and for
    /// jitter unless the stagger has its own; `default_each_ms` is the step when none is given.
    pub fn distribute(
        &self,
        doc: &Document,
        ids: &[String],
        seed: u64,
        default_each_ms: f64,
    ) -> Result<Distribution, ToolError> {
        let count = ids.len();
        if count == 0 || count > MAX_STAGGERED {
            return Err(ToolError(format!(
                "a stagger needs 1 to {MAX_STAGGERED} nodes"
            )));
        }
        let grid = self.grid(count)?;
        let spatial = self.spatial.unwrap_or(false);
        let metric = match self.metric.as_deref() {
            None => Metric::Euclid,
            Some(m) if grid.is_some() || spatial => Metric::parse(m).ok_or_else(|| {
                ToolError(format!(
                    "metric {m:?} is not euclid, manhattan or chebyshev"
                ))
            })?,
            Some(_) => {
                return Err(ToolError(
                    "metric needs `grid` or `columns` (or spatial: true)".into(),
                ));
            }
        };
        let axis = match self.axis.as_deref() {
            None => None,
            Some(a) => {
                Some(Axis::parse(a).ok_or_else(|| ToolError(format!("axis {a:?} is not x or y")))?)
            }
        };
        let each = self.each_ms.unwrap_or(default_each_ms);
        if !(each.is_finite() && (0.0..=MAX_EACH_MS).contains(&each)) {
            return Err(ToolError(format!("each_ms is from 0 to {MAX_EACH_MS}")));
        }
        let start = self.start_ms.unwrap_or(0.0);
        if !(start.is_finite() && (0.0..=600_000.0).contains(&start)) {
            return Err(ToolError(
                "start_ms of a stagger is from 0 to 600000".into(),
            ));
        }
        let common = StaggerSpec {
            step: Step::Each(each),
            from: self.origin(count)?,
            grid,
            metric,
            axis,
            ease: self.ease,
            reversed: self.reversed.unwrap_or(false),
            start: 0.0,
            jitter: None,
            seed: Some(self.seed.unwrap_or(seed)),
            positions: if spatial {
                Some(node_centres(doc, ids)?)
            } else {
                None
            },
        };
        let timed = StaggerSpec {
            start,
            jitter: self.jitter()?,
            ..common.clone()
        };
        let values = match self.range {
            None => None,
            Some([a, b]) if a.is_finite() && b.is_finite() => Some(stagger_values(
                count,
                &StaggerSpec {
                    step: Step::Range(a, b),
                    ..common
                },
            )),
            Some(_) => return Err(ToolError("range is two finite numbers".into())),
        };
        Ok(Distribution {
            offsets_ms: stagger_offsets_ms(count, &timed),
            values,
        })
    }
}

/// The middle of each node in one shared coordinate space (the page's, through each parent's
/// offset).
fn node_centres(doc: &Document, ids: &[String]) -> Result<Vec<(f64, f64)>, ToolError> {
    ids.iter()
        .map(|raw| {
            let id = NodeId::parse(raw).map_err(|e| ToolError(e.to_string()))?;
            let node = doc
                .node(&id)
                .ok_or_else(|| ToolError(format!("no node {id}")))?;
            let (x, y) =
                origin_of(doc, &id).ok_or_else(|| ToolError(format!("{id} is not in a page")))?;
            let (w, h) = extent_of(node);
            Ok((x + node.x + w / 2.0, y + node.y + h / 2.0))
        })
        .collect()
}

/// The `stagger` schema shared by `animate_preset` and `sequence`.
pub fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "description": "How a group of nodes is spread over time (and, with range, over a value). Every field is optional.",
        "properties": {
            "each_ms": {"type": "number", "minimum": 0, "maximum": MAX_EACH_MS, "description": "Delay per unit of distance between one node and the next (default 120; 40 to 120 reads well for 3 or more items)."},
            "range": {"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2, "description": "A value stagger: [near, far] spreads these over the distances, and each node's move intensity is multiplied by its value (e.g. [1, 0.6] makes the far nodes pop 40 percent less)."},
            "from": {"description": "Where the wave starts: first|last|center|edges|random, a node index, or {\"point\": [fx, fy]} as fractions across the group.", "anyOf": [{"type": "string", "enum": ["first", "last", "center", "edges", "random"]}, {"type": "integer", "minimum": 0}, {"type": "object", "properties": {"point": {"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2}}, "required": ["point"], "additionalProperties": false}]},
            "grid": {"type": "array", "items": {"type": "integer", "minimum": 1}, "minItems": 2, "maxItems": 2, "description": "[columns, rows]: the nodes are a grid read row by row."},
            "columns": {"type": "integer", "minimum": 1, "maximum": MAX_STAGGERED, "description": "Shorthand for grid with as many rows as needed."},
            "metric": {"type": "string", "enum": ["euclid", "manhattan", "chebyshev"], "description": "With grid or spatial: how distance is measured (default euclid)."},
            "axis": {"type": "string", "enum": ["x", "y"], "description": "Only the distance along this axis counts. Nodes before the origin would start early: all delays are shifted so the earliest is 0."},
            "ease": {"type": "string", "description": "Shapes the distribution (out_* front-loads the wave), the same names as the move ease."},
            "reversed": {"type": "boolean", "description": "The farthest goes first."},
            "start_ms": {"type": "number", "minimum": 0, "description": "Added to every delay."},
            "jitter_ms": {"description": "Random change to each delay, seeded: one size, or [size at the first, size at the last]. A delay pushed below 0 shifts all so the earliest is 0.", "anyOf": [{"type": "number", "minimum": 0}, {"type": "array", "items": {"type": "number", "minimum": 0}, "minItems": 2, "maxItems": 2}]},
            "seed": {"type": "integer", "minimum": 0, "description": "Seeds random order and jitter (default: the move's seed)."},
            "spatial": {"type": "boolean", "description": "Measure distances between the nodes' real centres in units of the nearest pair, so a cluster fans out by where its nodes are."}
        }
    })
}
