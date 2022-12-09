use crate::{
    component::actor::{
        facing::FacingActorComponent,
        velocity::VelocityActorComponent,
    },
    entity::actor::Actor,
};
use std::{
    f32::consts::{
        FRAC_PI_2,
        PI,
    },
    time::Duration,
};
use voxbrix_common::math::Vec3;
use winit::event::{
    ElementState,
    KeyboardInput,
    VirtualKeyCode,
};

const UP_VECTOR: Vec3<f32> = Vec3::new([0.0, 0.0, 1.0]);
const SAFE_FRAC_PI_2: f32 = FRAC_PI_2 - 0.0001;
const PI_2: f32 = PI * 2.0;

pub struct DirectControl {
    actor: Actor,
    move_left: f32,
    move_right: f32,
    move_forward: f32,
    move_backward: f32,
    move_up: f32,
    move_down: f32,
    rotate_horizontal: f32,
    rotate_vertical: f32,
    speed: f32,
    sensitivity: f32,
}

impl DirectControl {
    pub fn new(actor: Actor, speed: f32, sensitivity: f32) -> Self {
        Self {
            actor,
            move_left: 0.0,
            move_right: 0.0,
            move_forward: 0.0,
            move_backward: 0.0,
            move_up: 0.0,
            move_down: 0.0,
            rotate_horizontal: 0.0,
            rotate_vertical: 0.0,
            speed,
            sensitivity,
        }
    }

    pub fn process_keyboard(&mut self, input: &KeyboardInput) -> bool {
        // TODO use scancodes?
        let KeyboardInput {
            state,
            virtual_keycode,
            ..
        } = input;
        let key = if let Some(vc) = virtual_keycode {
            vc
        } else {
            return false;
        };
        let amount = if *state == ElementState::Pressed {
            1.0
        } else {
            0.0
        };
        match key {
            VirtualKeyCode::W | VirtualKeyCode::Up => {
                self.move_forward = amount;
                true
            },
            VirtualKeyCode::S | VirtualKeyCode::Down => {
                self.move_backward = amount;
                true
            },
            VirtualKeyCode::A | VirtualKeyCode::Left => {
                self.move_left = amount;
                true
            },
            VirtualKeyCode::D | VirtualKeyCode::Right => {
                self.move_right = amount;
                true
            },
            VirtualKeyCode::Space => {
                self.move_up = amount;
                true
            },
            VirtualKeyCode::LShift => {
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

    pub fn process(
        &mut self,
        dt: Duration,
        velocity_component: &mut VelocityActorComponent,
        facing_component: &mut FacingActorComponent,
    ) {
        // TODO add actor instead?
        let actor_facing = facing_component.get_mut(&self.actor).unwrap();
        let actor_velocity = velocity_component.get_mut(&self.actor).unwrap();

        let dt = dt.as_secs_f32();
        actor_facing.yaw += self.rotate_horizontal * self.sensitivity * dt;
        actor_facing.pitch += -self.rotate_vertical * self.sensitivity * dt;

        self.rotate_horizontal = 0.0;
        self.rotate_vertical = 0.0;

        if actor_facing.pitch < -SAFE_FRAC_PI_2 {
            actor_facing.pitch = -SAFE_FRAC_PI_2;
        } else if actor_facing.pitch > SAFE_FRAC_PI_2 {
            actor_facing.pitch = SAFE_FRAC_PI_2;
        }

        if actor_facing.yaw.abs() > PI_2 {
            actor_facing.yaw = actor_facing.yaw - (actor_facing.yaw / PI_2).trunc() * PI_2;
        }

        if actor_facing.yaw > PI {
            actor_facing.yaw = actor_facing.yaw - PI_2;
        } else if actor_facing.yaw < -PI {
            actor_facing.yaw = actor_facing.yaw + PI_2;
        }

        actor_velocity.vector = actor_facing
            .forward_right()
            .and_then(|(forward, right)| {
                let direction = forward * (self.move_forward - self.move_backward)
                    + right * (self.move_right - self.move_left)
                    + UP_VECTOR * (self.move_up - self.move_down);

                Some(direction.normalize()? * self.speed)
            })
            .unwrap_or([0.0, 0.0, 0.0].into());
    }
}
