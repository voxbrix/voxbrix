use ahash::AHashMap;
use voxbrix_common::entity::chunk::Chunk;

#[derive(Clone, Copy)]
enum Action {
    UpdateFull,
    UpdateBlocks {
        x_min: u16,
        x_max: u16,
        y_min: u16,
        y_max: u16,
        z_min: u16,
        z_max: u16,
    },
}

impl Action {
    fn override_with(self, other: Self) -> Self {
        match (self, other) {
            (Self::UpdateFull, _) | (_, Self::UpdateFull) => Self::UpdateFull,
            (
                Self::UpdateBlocks {
                    x_min,
                    x_max,
                    y_min,
                    y_max,
                    z_min,
                    z_max,
                },
                Self::UpdateBlocks {
                    x_min: new_x_min,
                    x_max: new_x_max,
                    y_min: new_y_min,
                    y_max: new_y_max,
                    z_min: new_z_min,
                    z_max: new_z_max,
                },
            ) => {
                Self::UpdateBlocks {
                    x_min: x_min.min(new_x_min),
                    x_max: x_max.max(new_x_max),
                    y_min: y_min.min(new_y_min),
                    y_max: y_max.max(new_y_max),
                    z_min: z_min.min(new_z_min),
                    z_max: z_max.max(new_z_max),
                }
            },
        }
    }
}

type PlayerDistance = i64;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Priority(u8);

impl Priority {
    const ADD: Self = Self(2);
    const NONE: Self = Self(0);
    const REMOVED_NEIGHBOR: Self = Self(1);
    const UPDATE: Self = Self(3);
}

#[derive(Clone, Copy)]
pub enum Procedure {
    ComputeSkyLight,
    BuildPolygons,
}

pub struct ComputeContext<'a> {
    pub procedure: Procedure,
    pub queue: &'a mut (dyn Iterator<Item = Chunk> + Send),
    add_light_affected_chunk: &'a mut (dyn FnMut(Chunk)),
}

impl<'a> ComputeContext<'a> {
    pub fn light_changed(&mut self, chunk: Chunk) {
        (self.add_light_affected_chunk)(chunk);
    }
}

pub struct ChunkRenderPipelineSystem {
    sky_light_queue: AHashMap<Chunk, (Action, Priority)>,
    sky_light_sort_buffer: Vec<(Chunk, PlayerDistance, Priority)>,
    build_polygons_queue: AHashMap<Chunk, (Action, Priority)>,
    build_polygons_sort_buffer: Vec<(Chunk, PlayerDistance, Priority)>,
}

impl ChunkRenderPipelineSystem {
    pub fn new() -> Self {
        Self {
            sky_light_queue: AHashMap::new(),
            sky_light_sort_buffer: Vec::new(),
            build_polygons_queue: AHashMap::new(),
            build_polygons_sort_buffer: Vec::new(),
        }
    }

    fn add_to_the_start(&mut self, chunk: Chunk, new_action: Action, new_priority: Priority) {
        let (action, priority) = self
            .sky_light_queue
            .get(&chunk)
            .map(|(action, priority)| {
                (
                    action.override_with(new_action),
                    (*priority).max(new_priority),
                )
            })
            .unwrap_or((new_action, new_priority));

        self.sky_light_queue.insert(chunk, (action, priority));
    }

    pub fn chunk_added(&mut self, chunk: Chunk) {
        self.add_to_the_start(chunk, Action::UpdateFull, Priority::ADD);
    }

    // TODO use UpdateBlocks
    pub fn chunk_updated(&mut self, chunk: Chunk) {
        self.add_to_the_start(chunk, Action::UpdateFull, Priority::UPDATE);
    }

    pub fn chunk_removed(&mut self, chunk: Chunk, chunk_exists: impl Fn(&Chunk) -> bool) {
        self.sky_light_queue.remove(&chunk);
        self.build_polygons_queue.remove(&chunk);

        let neighbor_chunks = [
            [-1, 0, 0],
            [1, 0, 0],
            [0, -1, 0],
            [0, 1, 0],
            [0, 0, -1],
            [0, 0, 1],
        ]
        .into_iter()
        .filter_map(|offset| chunk.checked_add(offset))
        .filter(chunk_exists);

        for chunk in neighbor_chunks {
            self.add_to_the_start(chunk, Action::UpdateFull, Priority::REMOVED_NEIGHBOR);
        }
    }

    pub fn compute_next(&mut self, player_chunk: Chunk, compute: impl FnOnce(ComputeContext<'_>)) {
        fill_chunk_queue(
            self.sky_light_queue.iter(),
            &mut self.sky_light_sort_buffer,
            &player_chunk,
        );

        fill_chunk_queue(
            self.build_polygons_queue.iter(),
            &mut self.build_polygons_sort_buffer,
            &player_chunk,
        );

        let procedure = match (
            self.build_polygons_sort_buffer.first(),
            self.sky_light_sort_buffer.first(),
        ) {
            (None, None) => None,
            (Some(_), None) => Some(Procedure::BuildPolygons),
            (None, Some(_)) => Some(Procedure::ComputeSkyLight),
            (Some((_, player_dist_1, priority_1)), Some((_, player_dist_2, priority_2))) => {
                let ordering = priority_1
                    .cmp(priority_2)
                    .reverse()
                    .then(player_dist_1.cmp(player_dist_2));
                // Ones not already calculated go first
                // Less player distance is the priority otherwise
                if ordering.is_le() {
                    Some(Procedure::BuildPolygons)
                } else {
                    Some(Procedure::ComputeSkyLight)
                }
            },
        };

        if let Some(procedure) = procedure {
            match procedure {
                Procedure::ComputeSkyLight => {
                    let mut queue = self
                        .sky_light_sort_buffer
                        .iter()
                        .map(|(chunk, _, _)| *chunk)
                        .take(rayon::current_num_threads());
                    let queue_copy = queue.clone();
                    let mut add_light_affected_chunk = |chunk: Chunk| {
                        self.sky_light_queue
                            .insert(chunk, (Action::UpdateFull, Priority::NONE));
                    };
                    let context = ComputeContext {
                        procedure,
                        queue: &mut queue,
                        add_light_affected_chunk: &mut add_light_affected_chunk,
                    };

                    compute(context);

                    for chunk in queue_copy {
                        let action_priority = self.sky_light_queue.remove(&chunk).unwrap();
                        self.build_polygons_queue.insert(chunk, action_priority);
                    }
                },
                Procedure::BuildPolygons => {
                    let mut queue = self
                        .build_polygons_sort_buffer
                        .iter()
                        .map(|(chunk, _, _)| *chunk)
                        .take(1);
                    let queue_copy = queue.clone();
                    let mut add_light_affected_chunk = |_: Chunk| {};
                    let context = ComputeContext {
                        procedure,
                        queue: &mut queue,
                        add_light_affected_chunk: &mut add_light_affected_chunk,
                    };

                    compute(context);

                    for chunk in queue_copy {
                        self.build_polygons_queue.remove(&chunk);
                    }
                },
            }
        }
    }
}

fn fill_chunk_queue<'a>(
    queue: impl Iterator<Item = (&'a Chunk, &'a (Action, Priority))>,
    sort_buffer: &mut Vec<(Chunk, PlayerDistance, Priority)>,
    player_chunk: &Chunk,
) {
    let iter_with_sort_params = queue.map(|(chunk, (_action, priority))| {
        let sum = [
            player_chunk.position[0] - chunk.position[0],
            player_chunk.position[1] - chunk.position[1],
            player_chunk.position[2] - chunk.position[2],
        ]
        .map(|i| (i as i64).pow(2))
        .iter()
        .sum();

        (*chunk, sum, *priority)
    });

    sort_buffer.clear();
    sort_buffer.extend(iter_with_sort_params);
    sort_buffer.sort_unstable_by(|c1, c2| {
        let (_, player_dist_1, priority_1) = c1;
        let (_, player_dist_2, priority_2) = c2;

        // Higher priority go first
        // Less player distance is the priority otherwise
        priority_1
            .cmp(priority_2)
            .reverse()
            .then(player_dist_1.cmp(&player_dist_2))
    });
}
