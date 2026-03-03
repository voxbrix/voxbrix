use std::{
    f32::consts::PI,
    time::Duration,
};
use voxbrix_common::{
    component::actor::orientation::Orientation,
    math::{
        Directions,
        QuatF32,
        Vec3F32,
    },
};
use winit::{
    event::{
        ElementState,
        KeyEvent,
    },
    keyboard::{
        KeyCode,
        PhysicalKey,
    },
};

const PITCH_CLAMP_ANGLE: f32 = 0.001;

pub struct PlayerInput {
    own_move: [bool; 6],
    rotate_horizontal: f32,
    rotate_vertical: f32,
    speed: f32,
    sensitivity: f32,
}

impl PlayerInput {
    pub fn new(speed: f32, sensitivity: f32) -> Self {
        Self {
            own_move: [false; 6],
            rotate_horizontal: 0.0,
            rotate_vertical: 0.0,
            speed,
            sensitivity,
        }
    }

    pub fn process_keyboard(&mut self, input: &KeyEvent) -> bool {
        // TODO use scancodes?
        let KeyEvent {
            state,
            physical_key,
            ..
        } = input;
        let key = match physical_key {
            PhysicalKey::Code(c) => c,
            PhysicalKey::Unidentified(_) => return false,
        };

        let pressed = *state == ElementState::Pressed;

        match key {
            KeyCode::KeyS | KeyCode::ArrowDown => {
                self.own_move[0] = pressed;
                true
            },
            KeyCode::KeyW | KeyCode::ArrowUp => {
                self.own_move[1] = pressed;
                true
            },
            KeyCode::KeyA | KeyCode::ArrowLeft => {
                self.own_move[2] = pressed;
                true
            },
            KeyCode::KeyD | KeyCode::ArrowRight => {
                self.own_move[3] = pressed;
                true
            },
            KeyCode::ShiftLeft => {
                self.own_move[4] = pressed;
                true
            },
            KeyCode::Space => {
                self.own_move[5] = pressed;
                true
            },
            _ => false,
        }
    }

    pub fn process_mouse(&mut self, horizontal: f32, vertical: f32) {
        self.rotate_horizontal = horizontal;
        self.rotate_vertical = vertical;
    }

    /// Take accumulated rotation as Orientation change.
    pub fn modify_orientation(&mut self, dt: Duration, orientation: &mut Orientation) {
        let dt = dt.as_secs_f32();
        let d_yaw = self.rotate_horizontal * self.sensitivity * dt;
        let d_pitch = self.rotate_vertical * self.sensitivity * dt;

        self.rotate_horizontal = 0.0;
        self.rotate_vertical = 0.0;

        orientation.rotation = QuatF32::from_axis_angle(Vec3F32::UP, d_yaw) * orientation.rotation;
        let pitch = orientation.forward().angle_between(Vec3F32::UP);
        let d_pitch =
            (d_pitch + pitch).clamp(PITCH_CLAMP_ANGLE, const { PI - PITCH_CLAMP_ANGLE }) - pitch;
        orientation.rotation =
            QuatF32::from_axis_angle(orientation.right(), d_pitch) * orientation.rotation;

        orientation.rotation = orientation.rotation.normalize();
    }

    pub fn velocity(&self, actor_orientation: Orientation) -> Vec3F32 {
        let mut forward = actor_orientation.forward();
        forward[2] = 0.0;

        let forward = forward.normalize();

        let mut movement = [0.0; 3];

        for (i, is_moving) in self.own_move.into_iter().enumerate() {
            let axis = i / 2;
            let sign = (i % 2 * 2) as f32 - 1.0;
            if is_moving {
                movement[axis] += sign;
            }
        }

        let movement = Vec3F32::from_array(movement);

        let direction = if forward.is_nan() {
            Vec3F32::new(0.0, 0.0, movement[2])
        } else {
            let right = Vec3F32::UP.cross(forward);

            forward * movement[0] + right * movement[1] + Vec3F32::UP * movement[2]
        };

        Some(direction.normalize())
            .filter(|n| !n.is_nan())
            .map(|d| d * self.speed)
            .unwrap_or(Vec3F32::ZERO)
    }
}
