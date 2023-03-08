use color_eyre::eyre::Result;
use glam::{vec3, Quat, Vec3};
use input_event_codes::BTN_LEFT;
use manifest_dir_macros::directory_relative_path;
use map_range::MapRange;
use mint::{Vector2, Vector3};
use stardust_xr_fusion::{
	client::{Client, FrameInfo, RootHandler},
	core::values::Transform,
	drawable::{MaterialParameter, Model, ResourceID},
	fields::BoxField,
	input::InputDataType,
	items::{
		panel::{self, PanelItem, PanelItemHandler, PanelItemInitData, ToplevelInfo},
		Item, ItemAcceptor, ItemAcceptorHandler,
	},
	node::NodeError,
	spatial::Spatial,
	HandlerWrapper, Mutex,
};
use stardust_xr_molecules::{
	keyboard::{KeyboardPanelHandler, KeyboardPanelRelay},
	touch_plane::TouchPlane,
	DebugSettings, VisualDebug,
};
use std::sync::{Arc, Weak};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
	color_eyre::install()?;
	let (client, event_loop) = Client::connect_with_async_loop().await?;
	client.set_base_prefixes(&[directory_relative_path!("res")]);

	let (poltergeist, _item_acceptor_handler) = Poltergeist::new(&client)?;
	client.wrap_root_raw(&poltergeist)?;

	tokio::select! {
		_ = tokio::signal::ctrl_c() => (),
		e = event_loop => e??,
	}
	Ok(())
}

struct CapturedItem {
	uid: String,
	toplevel_info: Option<ToplevelInfo>,
	_keyboard_relay: KeyboardPanelRelay,
}

type AcceptorHandler = HandlerWrapper<ItemAcceptor<PanelItem>, Poltergeist>;
struct Poltergeist {
	_self_ref: Weak<Mutex<Poltergeist>>,
	root: Spatial,
	bound_field: BoxField,
	model: Model,
	captured: Option<HandlerWrapper<PanelItem, CapturedItem>>,
	touch_plane: TouchPlane,
}
const TOUCH_PLANE_WIDTH: f32 = 0.403122;
const TOUCH_PLANE_HEIGHT: f32 = 0.313059;
impl Poltergeist {
	fn new(client: &Arc<Client>) -> Result<(Arc<Mutex<Self>>, AcceptorHandler), NodeError> {
		let root = Spatial::create(client.get_root(), Transform::default(), true)?;
		let bound_field = BoxField::create(
			&root,
			Transform::from_position([0.0, 0.227573, -0.084663]),
			[0.471, 0.46, 0.168],
		)?;
		let model = Model::create(
			&root,
			Transform::default(),
			&ResourceID::new_namespaced("poltergeist", "crt"),
		)?;
		let acceptor = ItemAcceptor::create(&root, Transform::default(), &bound_field)?;
		let mut touch_plane = TouchPlane::new(
			&root,
			Transform::from_position([0.0, 0.268524, 0.0]),
			[TOUCH_PLANE_WIDTH, TOUCH_PLANE_HEIGHT],
			0.172038,
		)?;
		// touch_plane.input_handler().set_transform(None, Transform::from_position_scale([TOUCH_PLANE_WIDTH * -0.5, TOUCH_PLANE_HEIGHT * 0.5, 0.0], [TOUCH_PLANE_WIDTH, TOUCH_PLANE_HEIGHT,]))
		touch_plane.set_debug(Some(DebugSettings::default()));
		let poltergeist = Arc::new_cyclic(|weak| {
			Mutex::new(Poltergeist {
				_self_ref: weak.clone(),
				root,
				bound_field,
				model,
				captured: None,
				touch_plane,
			})
		});
		Ok((poltergeist.clone(), acceptor.wrap_raw(poltergeist)?))
	}
}
impl RootHandler for Poltergeist {
	fn frame(&mut self, _info: FrameInfo) {
		self.touch_plane.update();

		let Some(captured_item) = self.captured.as_mut() else {return};
		let captured_item_info = captured_item.lock_wrapped();
		let Some(toplevel_info) = captured_item_info.toplevel_info.as_ref() else {return};
		if self.touch_plane.touch_started() {
			println!("touch started");
			let _ = captured_item.node().pointer_button(BTN_LEFT!(), true);
		}
		let touch_point = self
			.touch_plane
			.interacting_inputs()
			.into_iter()
			.filter_map(|i| match &i.input {
				InputDataType::Pointer(p) => {
					let normal = vec3(0.0, 0.0, -1.0);
					let denom = Vec3::from(p.direction()).dot(normal);
					if denom.abs() <= 0.0001 {
						return None;
					}
					let t = -Vec3::from(p.origin).dot(normal) / denom;
					if t < 0.0 {
						return None;
					}
					Some(Vector3::from(
						Vec3::from(p.origin) + (Vec3::from(p.direction()) * t),
					))
				}
				InputDataType::Hand(h) => Some(dbg!(h.index.tip.position)),
				InputDataType::Tip(t) => Some(t.origin),
			})
			.reduce(|a, b| if a.z < b.z { a } else { b })
			.map(|v| {
				let half_width = TOUCH_PLANE_WIDTH * 0.5;
				let half_height = TOUCH_PLANE_HEIGHT * 0.5;
				Vector2::from([
					v.x.map_range(-half_width..half_width, 0.0..toplevel_info.size.x as f32),
					v.y.map_range(half_height..-half_height, 0.0..toplevel_info.size.y as f32),
				])
			});
		if let Some(touch_point) = touch_point {
			// dbg!(toplevel_info.size);
			// dbg!(touch_point);
			let _ = captured_item.node().pointer_motion(touch_point);
		}

		if self.touch_plane.touch_stopped() {
			println!("touch stopped");
			let _ = captured_item.node().pointer_button(BTN_LEFT!(), false);
		}
	}
}
const SCREEN_MATERIAL_INDEX: u32 = 3;
impl ItemAcceptorHandler<PanelItem> for Poltergeist {
	fn captured(&mut self, uid: &str, item: PanelItem, init_data: PanelItemInitData) {
		if let Some(captured) = self.captured.take() {
			let _ = captured.node().release();
		}
		println!("Captured {uid} into Poltergeist");

		let _ = item.set_transform(
			Some(&self.root),
			Transform::from_position_rotation_scale(
				[0.0, 0.268524, 0.05],
				Quat::IDENTITY,
				[1.0; 3],
			),
		);
		let _ = item.configure_toplevel(
			Some([640, 480].into()),
			&[panel::State::Activated, panel::State::Maximized],
			None,
		);
		let _ = item.apply_toplevel_material(&self.model, SCREEN_MATERIAL_INDEX);
		let _ = self.model.set_material_parameter(
			SCREEN_MATERIAL_INDEX,
			"alpha_min",
			MaterialParameter::Float(1.0),
		);

		let keyboard_relay = KeyboardPanelHandler::create(
			&self.root,
			Transform::from_position([0.070582, 0.052994, 0.000832]),
			&self.bound_field,
			&item,
		)
		.unwrap();
		item.pointer_set_active(true).unwrap();
		self.captured.replace(
			item.wrap(CapturedItem {
				uid: uid.to_string(),
				toplevel_info: init_data.toplevel,
				_keyboard_relay: keyboard_relay,
			})
			.unwrap(),
		);
	}
	fn released(&mut self, uid: &str) {
		if self.captured.is_some() && self.captured.as_ref().unwrap().lock_wrapped().uid == uid {
			self.captured.take();
		}
	}
}
impl PanelItemHandler for CapturedItem {
	fn commit_toplevel(&mut self, state: Option<ToplevelInfo>) {
		self.toplevel_info = state;
	}
}
