use std::{
    f32::consts::{
        FRAC_PI_2,
        PI,
    },
    time::Duration,
};
use voxbrix_common::{
    component::actor::orientation::Orientation,
    math::{
        Directions,
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

const SAFE_FRAC_PI_2: f32 = FRAC_PI_2 - 0.0001;
const PI_2: f32 = PI * 2.0;

pub struct PlayerInput {
    move_left: f32,
    move_right: f32,
    move_forward: f32,
    move_backward: f32,
    move_up: f32,
    move_down: f32,
    yaw: f32,
    pitch: f32,
    rotate_horizontal: f32,
    rotate_vertical: f32,
    speed: f32,
    sensitivity: f32,
}

impl PlayerInput {
    pub fn new(speed: f32, sensitivity: f32) -> Self {
        Self {
            move_left: 0.0,
            move_right: 0.0,
            move_forward: 0.0,
            move_backward: 0.0,
            move_up: 0.0,
            move_down: 0.0,
            yaw: 0.0,
            pitch: 0.0,
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

        let amount = if *state == ElementState::Pressed {
            1.0
        } else {
            0.0
        };

        match key {
            KeyCode::KeyW | KeyCode::ArrowUp => {
                self.move_forward = amount;
                true
            },
            KeyCode::KeyS | KeyCode::ArrowDown => {
                self.move_backward = amount;
                true
            },
            KeyCode::KeyA | KeyCode::ArrowLeft => {
                self.move_left = amount;
                true
            },
            KeyCode::KeyD | KeyCode::ArrowRight => {
                self.move_right = amount;
                true
            },
            KeyCode::Space => {
                self.move_up = amount;
                true
            },
            KeyCode::ShiftLeft => {
                self.move_down = amount;
                true
            },
            _ => false,
        }
    }

    pub fn process_mouse(&mut self, horizontal: f32, vertical: f32) {
        self.rotate_horizontal = horizontal;
        self.rotate_vertical = vertical;
    }

    /// Take accumulated rotation as Orientation.
    pub fn take_orientation(&mut self, dt: Duration) -> Orientation {
        let dt = dt.as_secs_f32();
        self.yaw += self.rotate_horizontal * self.sensitivity * dt;
        self.pitch += -self.rotate_vertical * self.sensitivity * dt;

        self.rotate_horizontal = 0.0;
        self.rotate_vertical = 0.0;

        self.pitch = self.pitch.clamp(-SAFE_FRAC_PI_2, SAFE_FRAC_PI_2);

        if self.yaw.abs() > PI_2 {
            self.yaw = self.yaw - (self.yaw / PI_2).trunc() * PI_2;
        }

        if self.yaw > PI {
            self.yaw -= PI_2;
        } else if self.yaw < -PI {
            self.yaw += PI_2;
        }

        Orientation::from_yaw_pitch(self.yaw, self.pitch)
    }

    pub fn velocity(&self, actor_orientation: Orientation) -> Vec3F32 {
        let mut forward = actor_orientation.forward();
        forward[2] = 0.0;

        let forward = forward.normalize();

        let direction = if forward.is_nan() {
            Vec3F32::UP * (self.move_up - self.move_down)
        } else {
            let right = Vec3F32::UP.cross(forward);

            forward * (self.move_forward - self.move_backward)
                + right * (self.move_right - self.move_left)
                + Vec3F32::UP * (self.move_up - self.move_down)
        };

        Some(direction.normalize())
            .filter(|n| !n.is_nan())
            .map(|d| d * self.speed)
            .unwrap_or(Vec3F32::ZERO)
    }
}
