use ahash::AHashMap;
use voxbrix_common::entity::chunk::Chunk;

#[derive(Clone, Copy)]
pub enum Action {
    Add,
    Update,
    Remove,
}

impl From<Action> for Priority {
    fn from(value: Action) -> Priority {
        match value {
            Action::Add => Priority::ADD,
            Action::Update => Priority::UPDATE,
            Action::Remove => Priority::REMOVE,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Priority(u8);

impl Priority {
    const ADD: Priority = Priority(2);
    const LOW_PRIORITY: Priority = Priority(0);
    const REMOVE: Priority = Priority(1);
    const UPDATE: Priority = Priority(3);
}

pub struct RenderPriorityChunkComponent {
    queue: AHashMap<Chunk, Action>,
    priorities: AHashMap<Chunk, Priority>,
}

impl RenderPriorityChunkComponent {
    pub fn new() -> Self {
        Self {
            queue: AHashMap::new(),
            priorities: AHashMap::new(),
        }
    }

    fn new_action(&mut self, chunk: Chunk, action: Action) {
        let priority = action.into();
        let priority = self
            .priorities
            .get(&chunk)
            .copied()
            .unwrap_or(priority)
            .max(priority);
        // Do the latest action, only possibly increase priority.
        // Update/Add are compatible for now.
        // Remove overrides other ones.
        // TODO Update / Add might not be compatible in the future.
        self.queue.insert(chunk, action);
        self.priorities.insert(chunk, priority);
    }

    pub fn chunk_added(&mut self, chunk: &Chunk) {
        self.new_action(*chunk, Action::Add);
    }

    pub fn chunk_updated(&mut self, chunk: &Chunk) {
        self.new_action(*chunk, Action::Update);
    }

    pub fn chunk_removed(&mut self, chunk: &Chunk) {
        self.new_action(*chunk, Action::Remove);
    }

    pub fn drain_queue<'a>(&'a mut self) -> impl Iterator<Item = (Chunk, Action)> + 'a {
        self.queue.drain()
    }

    pub fn get_priority(&self, chunk: &Chunk) -> Priority {
        self.priorities
            .get(chunk)
            .copied()
            .unwrap_or(Priority::LOW_PRIORITY)
    }

    pub fn finish_chunks(&mut self, chunks: impl Iterator<Item = Chunk>) {
        for chunk in chunks {
            // Should be fine since finish_chunks must happen after drain_queue
            // which empties the queue.
            self.priorities.remove(&chunk);
        }
    }
}

pub fn fill_chunk_queue<'a>(
    queue: impl Iterator<Item = &'a Chunk>,
    sort_buffer: &mut Vec<(Chunk, i64, Priority)>,
    render_priority_cc: &RenderPriorityChunkComponent,
    player_chunk: Chunk,
) {
    let iter_with_sort_params = queue.map(|chunk| {
        let sum = [
            player_chunk.position[0] - chunk.position[0],
            player_chunk.position[1] - chunk.position[1],
            player_chunk.position[2] - chunk.position[2],
        ]
        .map(|i| (i as i64).pow(2))
        .iter()
        .sum();

        let priority = render_priority_cc.get_priority(chunk);

        (*chunk, sum, priority)
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
