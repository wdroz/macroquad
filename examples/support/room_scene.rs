//! Shared public-API room contract; no window or Blender needed.
use macroquad::gltf::{GltfColliderShape, GltfScene};
use macroquad::math::{slide_sphere_solids, vec3, Aabb, Sphere, UniformGrid, Vec3};

pub const BYTES: &[u8] = include_bytes!("../assets/room.glb");
pub const RADIUS: f32 = 0.4;

pub struct Room {
    pub scene: GltfScene,
    pub spawn: Vec3,
    pub goal: Sphere,
    pub solids: Vec<Aabb>,
    // Indices address scene.colliders, not scene.nodes or the solid-only list.
    // None means every marker must remain a candidate.
    grid: Option<UniformGrid>,
}

impl Room {
    pub fn load() -> Self {
        let scene = GltfScene::from_bytes(BYTES).expect("committed room");
        assert_eq!(scene.nodes.len(), 10);
        assert_eq!(scene.colliders.len(), 5);
        assert_eq!(scene.spawns.len(), 1);
        assert_eq!(scene.marker_tags.len(), scene.nodes.len());
        let mut solids = Vec::new();
        for (tag, center, size) in [
            ("floor", vec3(0., -0.1, 0.), vec3(8., 0.2, 8.)),
            ("left", vec3(-4., 1., 0.), vec3(0.2, 2., 8.)),
            ("right", vec3(4., 1., 0.), vec3(0.2, 2., 8.)),
            ("back", vec3(0., 1., -4.), vec3(8., 2., 0.2)),
        ] {
            let markers: Vec<_> = scene
                .solid_colliders()
                .filter(|c| scene.marker_tag(c.node) == Some(tag))
                .collect();
            assert_eq!(markers.len(), 1, "solid tag {tag}");
            let collider = markers[0];
            assert!(scene.nodes[collider.node].mesh.is_none());
            let GltfColliderShape::Aabb(bounds) = collider.shape else {
                panic!("empty {tag} needs explicit box shape");
            };
            let expected = Aabb::from_center_size(center, size);
            assert!((bounds.min - expected.min).length() < 1e-5);
            assert!((bounds.max - expected.max).length() < 1e-5);
            solids.push(bounds);
        }
        let goals: Vec<_> = scene
            .trigger_colliders()
            .filter(|c| scene.marker_tag(c.node) == Some("goal"))
            .collect();
        assert_eq!(goals.len(), 1);
        let marker = goals[0];
        assert_eq!(
            scene.nodes[marker.node].name.as_deref(),
            Some("goal_trigger.001")
        );
        assert!(scene.nodes[marker.node].mesh.is_none());
        let GltfColliderShape::Sphere(goal) = marker.shape else {
            panic!("goal needs explicit sphere shape");
        };
        assert!((goal.radius - 0.6).abs() < 1e-5);
        assert!((goal.center - vec3(0., 0.4, -2.)).length() < 1e-5);
        let spawn = &scene.spawns[0];
        assert_eq!(spawn.name.as_deref(), Some("player_spawn.001"));
        assert_eq!(scene.marker_tag(spawn.node), Some("player"));
        assert!((spawn.position - vec3(0., 0.4, 2.)).length() < 1e-5);
        let spawn = spawn.position;
        let mut grid = UniformGrid::new(2.).unwrap();
        let mut complete = true;
        for (index, collider) in scene.colliders.iter().enumerate() {
            let Some((index, bounds)) = u32::try_from(index)
                .ok()
                .zip(collider.shape.bounding_aabb())
            else {
                complete = false;
                break;
            };
            complete &= grid.insert(index, bounds);
        }
        Self {
            scene,
            spawn,
            goal,
            solids,
            grid: complete.then_some(grid),
        }
    }

    /// Rebuild goal-query bounds after replacing world-space shape snapshots.
    /// A posed adaptation must preserve unchanged authored scene/collider data:
    /// derive collider_shape from that data for each pose, then replace query
    /// copies. Reusing an already posed collider would compound its pose delta.
    /// The example's complete slide-solid list is managed separately.
    pub fn rebuild_bounds(&mut self) {
        let snapshots: Option<Vec<_>> = self
            .scene
            .colliders
            .iter()
            .enumerate()
            .map(|(index, collider)| {
                Some((u32::try_from(index).ok()?, collider.shape.bounding_aabb()?))
            })
            .collect();
        self.grid = snapshots.map(|snapshots| {
            let mut grid = self
                .grid
                .take()
                .unwrap_or_else(|| UniformGrid::new(2.).unwrap());
            grid.rebuild(snapshots);
            grid
        });
        // A failed conversion disables this entire candidate filter. In
        // particular, never filter_map away an unrepresentable enclosure.
    }

    pub fn step(&self, center: Vec3, delta: Vec3) -> Vec3 {
        // The full tiny solid set also covers slide deflection/recovery.
        // Triggers are queried separately and never block the player.
        slide_sphere_solids(center, RADIUS, delta, &self.solids, &[], &[], &[])
    }

    pub fn reached(&self, center: Vec3) -> bool {
        let body = Sphere::new(center, RADIUS);
        let is_reached = |index: usize| {
            let collider = &self.scene.colliders[index];
            collider.trigger
                && self.scene.marker_tag(collider.node) == Some("goal")
                && collider.shape.overlaps_sphere(&body)
        };
        match (&self.grid, body.bounding_aabb()) {
            (Some(grid), Some(bounds)) => grid
                .query_aabb(&bounds)
                .into_iter()
                .any(|index| is_reached(index as usize)),
            // Invalid/unrepresentable query bounds also use complete candidates.
            _ => (0..self.scene.colliders.len()).any(is_reached),
        }
    }
}
