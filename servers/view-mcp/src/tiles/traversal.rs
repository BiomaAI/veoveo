use glam::{DMat4, DVec3, DVec4};

use super::{
    geo::region_to_ecef_volume,
    schema::{Refine, Tile, Tileset, VolumeKind},
};

const INSIDE_EPSILON: f64 = 1e-9;

#[derive(Debug, Clone, Copy)]
pub enum WorldVolume {
    Sphere {
        center: DVec3,
        radius: f64,
    },
    Obb {
        center: DVec3,
        half_axes: [DVec3; 3],
    },
}

impl WorldVolume {
    pub fn distance_to(self, point: DVec3) -> f64 {
        match self {
            Self::Sphere { center, radius } => (point - center).length() - radius,
            Self::Obb { center, half_axes } => {
                let delta = point - center;
                let mut closest = center;
                for axis in half_axes {
                    let length = axis.length();
                    if length > 1e-12 {
                        let direction = axis / length;
                        closest += direction * delta.dot(direction).clamp(-length, length);
                    }
                }
                (point - closest).length()
            }
        }
        .max(0.0)
    }

    pub fn bounding_sphere(self) -> (DVec3, f64) {
        match self {
            Self::Sphere { center, radius } => (center, radius),
            Self::Obb { center, half_axes } => {
                let radius = half_axes
                    .into_iter()
                    .map(|axis| axis.length_squared())
                    .sum::<f64>()
                    .sqrt();
                (center, radius)
            }
        }
    }

    fn transformed(self, transform: DMat4) -> Self {
        match self {
            Self::Sphere { center, radius } => Self::Sphere {
                center: transform.transform_point3(center),
                radius: radius * max_scale(transform),
            },
            Self::Obb { center, half_axes } => Self::Obb {
                center: transform.transform_point3(center),
                half_axes: half_axes.map(|axis| transform.transform_vector3(axis)),
            },
        }
    }
}

fn max_scale(transform: DMat4) -> f64 {
    transform
        .x_axis
        .truncate()
        .length()
        .max(transform.y_axis.truncate().length())
        .max(transform.z_axis.truncate().length())
}

pub const Y_UP_TO_Z_UP: DMat4 = DMat4::from_cols(
    DVec4::new(1.0, 0.0, 0.0, 0.0),
    DVec4::new(0.0, 0.0, 1.0, 0.0),
    DVec4::new(0.0, -1.0, 0.0, 0.0),
    DVec4::new(0.0, 0.0, 0.0, 1.0),
);

#[derive(Debug, Clone)]
pub struct TileNode {
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub depth: u32,
    pub geometric_error: f64,
    pub refine: Refine,
    pub content_uri: Option<String>,
    pub volume: WorldVolume,
    pub ecef_from_content: DMat4,
    pub ecef_from_tile: DMat4,
}

#[derive(Debug, Clone, Default)]
pub struct TileTree {
    pub nodes: Vec<TileNode>,
}

impl TileTree {
    pub fn build(tileset: &Tileset) -> Result<Self, TraversalError> {
        let mut tree = Self::default();
        build_node(
            &mut tree,
            &tileset.root,
            None,
            0,
            Refine::Replace,
            DMat4::IDENTITY,
            None,
        )?;
        Ok(tree)
    }

    pub fn graft(&mut self, parent: usize, tileset: &Tileset) -> Result<usize, TraversalError> {
        let parent_node = self
            .nodes
            .get(parent)
            .ok_or(TraversalError::InvalidParent)?;
        let index = build_node(
            self,
            &tileset.root,
            Some(parent),
            parent_node.depth + 1,
            parent_node.refine,
            parent_node.ecef_from_tile,
            Some(parent_node.volume),
        )?;
        self.nodes[parent].children.push(index);
        self.nodes[parent].content_uri = None;
        Ok(index)
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

#[allow(clippy::too_many_arguments)]
fn build_node(
    tree: &mut TileTree,
    tile: &Tile,
    parent: Option<usize>,
    depth: u32,
    inherited_refine: Refine,
    parent_transform: DMat4,
    parent_volume: Option<WorldVolume>,
) -> Result<usize, TraversalError> {
    let local = tile
        .transform
        .map(|matrix| DMat4::from_cols_array(&matrix))
        .unwrap_or(DMat4::IDENTITY);
    let transform = parent_transform * local;
    let volume = match tile.bounding_volume.kind() {
        Some(VolumeKind::Sphere([x, y, z, radius])) => WorldVolume::Sphere {
            center: DVec3::new(x, y, z),
            radius,
        }
        .transformed(transform),
        Some(VolumeKind::Box(value)) => WorldVolume::Obb {
            center: DVec3::new(value[0], value[1], value[2]),
            half_axes: [
                DVec3::new(value[3], value[4], value[5]),
                DVec3::new(value[6], value[7], value[8]),
                DVec3::new(value[9], value[10], value[11]),
            ],
        }
        .transformed(transform),
        Some(VolumeKind::Region(region)) => region_to_ecef_volume(&region),
        None => parent_volume.ok_or(TraversalError::MissingRootVolume)?,
    };
    let refine = tile.refine.unwrap_or(inherited_refine);
    let index = tree.nodes.len();
    tree.nodes.push(TileNode {
        parent,
        children: Vec::new(),
        depth,
        geometric_error: tile.geometric_error,
        refine,
        content_uri: tile.content_uri().map(ToOwned::to_owned),
        volume,
        ecef_from_content: transform * Y_UP_TO_Z_UP,
        ecef_from_tile: transform,
    });
    for child in &tile.children {
        let child_index = build_node(
            tree,
            child,
            Some(index),
            depth + 1,
            refine,
            transform,
            Some(volume),
        )?;
        tree.nodes[index].children.push(child_index);
    }
    Ok(index)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileReadiness {
    Empty,
    Pending,
    Ready,
    Failed,
}

impl TileReadiness {
    fn settled(self) -> bool {
        matches!(self, Self::Ready | Self::Failed)
    }

    fn loadable(self) -> bool {
        matches!(self, Self::Empty | Self::Pending)
    }
}

#[derive(Debug, Clone, Default)]
pub struct SelectionHistory {
    pub rendered: Vec<bool>,
    pub refined: Vec<bool>,
}

impl SelectionHistory {
    pub fn resize(&mut self, nodes: usize) {
        self.rendered.resize(nodes, false);
        self.refined.resize(nodes, false);
    }

    pub fn absorb(&mut self, selection: &Selection, nodes: usize) {
        self.rendered.clear();
        self.rendered.resize(nodes, false);
        for &tile in &selection.render {
            self.rendered[tile] = true;
        }
        self.refined = selection.refined.clone();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LoadPriority {
    Urgent,
    Descend,
    Normal,
    Preload,
}

#[derive(Debug, Clone, Copy)]
pub struct LoadRequest {
    pub tile: usize,
    pub priority: LoadPriority,
    pub key: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct SelectionParams {
    pub camera_position: DVec3,
    pub camera_forward: DVec3,
    pub focal_length_px: f64,
    pub max_screen_error_px: f64,
    pub detail_falloff_meters: f64,
    pub camera_height_meters: f64,
}

#[derive(Debug, Default)]
pub struct Selection {
    pub render: Vec<usize>,
    pub loads: Vec<LoadRequest>,
    pub refined: Vec<bool>,
    pub touched: Vec<bool>,
    pub covered: bool,
    pub detail_complete: bool,
    pub actual_max_screen_error_px: f64,
}

pub fn screen_space_error(geometric_error: f64, distance: f64, focal_length: f64) -> f64 {
    if distance <= INSIDE_EPSILON {
        f64::INFINITY
    } else {
        geometric_error * focal_length / distance
    }
}

pub fn select<F: Fn(usize) -> bool>(
    tree: &TileTree,
    readiness: &[TileReadiness],
    history: &SelectionHistory,
    culled: &F,
    params: SelectionParams,
) -> Selection {
    if tree.is_empty() || readiness.len() != tree.len() {
        return Selection::default();
    }
    let mut selection = Selection {
        refined: vec![false; tree.len()],
        touched: vec![false; tree.len()],
        ..Default::default()
    };
    let context = TraversalContext {
        tree,
        readiness,
        history,
        culled,
        params,
    };
    selection.covered = visit(&context, 0, &mut selection).covered;
    selection.detail_complete = selection.loads.is_empty();

    let mut queued = vec![false; tree.len()];
    for request in &selection.loads {
        queued[request.tile] = true;
    }
    for index in 0..selection.render.len() {
        let mut parent = tree.nodes[selection.render[index]].parent;
        while let Some(tile) = parent {
            selection.touched[tile] = true;
            if readiness[tile].loadable() && tree.nodes[tile].content_uri.is_some() && !queued[tile]
            {
                queued[tile] = true;
                let distance = selection_distance(&tree.nodes[tile], params);
                selection.loads.push(LoadRequest {
                    tile,
                    priority: LoadPriority::Preload,
                    key: load_key(&context, tile, distance),
                });
            }
            parent = tree.nodes[tile].parent;
        }
    }
    selection.loads.sort_by(|left, right| {
        (left.priority, left.key)
            .partial_cmp(&(right.priority, right.key))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    selection.actual_max_screen_error_px = selection
        .render
        .iter()
        .map(|&tile| {
            screen_space_error(
                tree.nodes[tile].geometric_error,
                selection_distance(&tree.nodes[tile], params),
                params.focal_length_px,
            )
        })
        .fold(0.0, f64::max);
    selection
}

struct TraversalContext<'a, F: Fn(usize) -> bool> {
    tree: &'a TileTree,
    readiness: &'a [TileReadiness],
    history: &'a SelectionHistory,
    culled: &'a F,
    params: SelectionParams,
}

struct VisitResult {
    covered: bool,
    rendered_last_frame: bool,
}

fn selection_distance(node: &TileNode, params: SelectionParams) -> f64 {
    node.volume
        .distance_to(params.camera_position)
        .max(params.camera_height_meters)
}

fn load_key<F: Fn(usize) -> bool>(
    context: &TraversalContext<'_, F>,
    tile: usize,
    distance: f64,
) -> f64 {
    let (center, _) = context.tree.nodes[tile].volume.bounding_sphere();
    let toward_tile = center - context.params.camera_position;
    let cosine = if toward_tile.length_squared() > 1e-12 {
        toward_tile
            .normalize()
            .dot(context.params.camera_forward)
            .clamp(-1.0, 1.0)
    } else {
        1.0
    };
    distance * (2.0 - cosine)
}

fn push_load<F: Fn(usize) -> bool>(
    context: &TraversalContext<'_, F>,
    selection: &mut Selection,
    tile: usize,
    distance: f64,
) {
    if context.readiness[tile].loadable() && context.tree.nodes[tile].content_uri.is_some() {
        selection.loads.push(LoadRequest {
            tile,
            priority: if distance <= INSIDE_EPSILON {
                LoadPriority::Urgent
            } else {
                LoadPriority::Normal
            },
            key: load_key(context, tile, distance),
        });
    }
}

fn visit<F: Fn(usize) -> bool>(
    context: &TraversalContext<'_, F>,
    tile: usize,
    selection: &mut Selection,
) -> VisitResult {
    let node = &context.tree.nodes[tile];
    selection.touched[tile] = true;
    let distance = selection_distance(node, context.params);
    let screen_error = screen_space_error(
        node.geometric_error,
        distance,
        context.params.focal_length_px,
    );
    let threshold = if context.params.detail_falloff_meters > 0.0 {
        let extra = (distance - context.params.camera_height_meters).max(0.0);
        context.params.max_screen_error_px * (1.0 + extra / context.params.detail_falloff_meters)
    } else {
        context.params.max_screen_error_px
    };
    let mut refine =
        !node.children.is_empty() && (screen_error > threshold || node.content_uri.is_none());
    if !refine
        && !node.children.is_empty()
        && context.history.refined.get(tile).copied().unwrap_or(false)
        && !context.readiness[tile].settled()
    {
        refine = true;
        push_load(context, selection, tile, distance);
    }

    if !refine {
        push_load(context, selection, tile, distance);
        if node.children.is_empty()
            && screen_error > threshold
            && let Some(request) = selection.loads.last_mut()
            && request.tile == tile
            && request.priority == LoadPriority::Normal
        {
            request.priority = LoadPriority::Descend;
        }
        if context.readiness[tile] == TileReadiness::Ready {
            selection.render.push(tile);
        }
        return VisitResult {
            covered: node.content_uri.is_none() || context.readiness[tile].settled(),
            rendered_last_frame: context.history.rendered.get(tile).copied().unwrap_or(false),
        };
    }

    selection.refined[tile] = true;
    if node.refine == Refine::Add {
        push_load(context, selection, tile, distance);
        if context.readiness[tile] == TileReadiness::Ready {
            selection.render.push(tile);
        }
        let mut rendered_last_frame = context.history.rendered.get(tile).copied().unwrap_or(false);
        for &child in &node.children {
            if (context.culled)(child) {
                continue;
            }
            rendered_last_frame |= visit(context, child, selection).rendered_last_frame;
        }
        return VisitResult {
            covered: node.content_uri.is_none() || context.readiness[tile].settled(),
            rendered_last_frame,
        };
    }

    let checkpoint = selection.render.len();
    let mut all_covered = true;
    let mut rendered_last_frame = false;
    for &child in &node.children {
        if (context.culled)(child) {
            continue;
        }
        let result = visit(context, child, selection);
        all_covered &= result.covered;
        rendered_last_frame |= result.rendered_last_frame;
    }
    if all_covered {
        return VisitResult {
            covered: true,
            rendered_last_frame,
        };
    }
    if rendered_last_frame {
        push_load(context, selection, tile, distance);
        return VisitResult {
            covered: true,
            rendered_last_frame: true,
        };
    }
    if context.readiness[tile] == TileReadiness::Ready {
        selection.render.truncate(checkpoint);
        selection.render.push(tile);
        return VisitResult {
            covered: true,
            rendered_last_frame: context.history.rendered.get(tile).copied().unwrap_or(false),
        };
    }
    selection.render.truncate(checkpoint);
    if context.readiness[tile].loadable() && node.content_uri.is_some() {
        selection.loads.push(LoadRequest {
            tile,
            priority: LoadPriority::Urgent,
            key: load_key(context, tile, distance),
        });
    }
    VisitResult {
        covered: false,
        rendered_last_frame: false,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TraversalError {
    #[error("root bounding volume is missing")]
    MissingRootVolume,
    #[error("external tileset parent index is invalid")]
    InvalidParent,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tiles::schema;

    fn fixture() -> TileTree {
        let tileset = schema::parse(
            br#"{
              "asset":{"version":"1.1"},
              "geometricError":100,
              "root":{
                "boundingVolume":{"sphere":[0,0,0,20]},
                "geometricError":20,
                "refine":"REPLACE",
                "content":{"uri":"root.glb"},
                "children":[
                  {"boundingVolume":{"sphere":[-10,0,0,10]},"geometricError":0,"content":{"uri":"left.glb"}},
                  {"boundingVolume":{"sphere":[10,0,0,10]},"geometricError":0,"content":{"uri":"right.glb"}}
                ]
              }
            }"#,
        )
        .unwrap();
        TileTree::build(&tileset).unwrap()
    }

    fn params() -> SelectionParams {
        SelectionParams {
            camera_position: DVec3::new(0.0, 0.0, 100.0),
            camera_forward: DVec3::NEG_Z,
            focal_length_px: 1_000.0,
            max_screen_error_px: 10.0,
            detail_falloff_meters: 0.0,
            camera_height_meters: 0.0,
        }
    }

    fn history(nodes: usize) -> SelectionHistory {
        SelectionHistory {
            rendered: vec![false; nodes],
            refined: vec![false; nodes],
        }
    }

    fn no_cull(_: usize) -> bool {
        false
    }

    #[test]
    fn pending_children_keep_ready_parent_as_covered_fallback() {
        let tree = fixture();
        let selection = select(
            &tree,
            &[
                TileReadiness::Ready,
                TileReadiness::Empty,
                TileReadiness::Empty,
            ],
            &history(tree.len()),
            &no_cull,
            params(),
        );
        assert!(selection.covered);
        assert_eq!(selection.render, vec![0]);
        assert_eq!(selection.loads.len(), 2);
        assert!(!selection.detail_complete);
    }

    #[test]
    fn ready_children_replace_parent() {
        let tree = fixture();
        let selection = select(
            &tree,
            &[TileReadiness::Ready; 3],
            &history(tree.len()),
            &no_cull,
            params(),
        );
        assert!(selection.covered);
        assert_eq!(selection.render, vec![1, 2]);
        assert!(selection.detail_complete);
    }

    #[test]
    fn distance_falloff_stops_far_refinement() {
        let tree = fixture();
        let ready = [TileReadiness::Ready; 3];
        let far = SelectionParams {
            camera_position: DVec3::new(0.0, 0.0, 1_000.0),
            ..params()
        };
        let unbounded = select(&tree, &ready, &history(tree.len()), &no_cull, far);
        assert_eq!(unbounded.render, vec![1, 2]);

        let bounded = select(
            &tree,
            &ready,
            &history(tree.len()),
            &no_cull,
            SelectionParams {
                detail_falloff_meters: 500.0,
                ..far
            },
        );
        assert_eq!(bounded.render, vec![0]);
    }

    #[test]
    fn camera_height_floor_stops_inside_volume_over_refinement() {
        let tree = fixture();
        let ready = [TileReadiness::Ready; 3];
        let inside = SelectionParams {
            camera_position: DVec3::ZERO,
            camera_forward: DVec3::NEG_Z,
            ..params()
        };
        let unfloored = select(&tree, &ready, &history(tree.len()), &no_cull, inside);
        assert_eq!(unfloored.render, vec![1, 2]);

        let floored = select(
            &tree,
            &ready,
            &history(tree.len()),
            &no_cull,
            SelectionParams {
                camera_height_meters: 2_000.0,
                ..inside
            },
        );
        assert_eq!(floored.render, vec![0]);
    }
}
