use rustc_hash::FxHashMap;
use stardust_xr_molecules::{
    fusion::{
        client::FrameInfo,
        core::values::Transform,
        drawable::{Model, ResourceID},
        fields::BoxField,
        items::{
            panel::{PanelItem, PanelItemHandler, PanelItemInitData, ToplevelInfo},
            ItemUI, ItemUIHandler,
        },
        node::{NodeError, NodeType},
        HandlerWrapper,
    },
    GrabData, Grabbable,
};
use std::sync::{Arc, Mutex};

pub struct PanelItemUIHandler {
    item_ui: ItemUI<PanelItem>,
    items: FxHashMap<String, HandlerWrapper<PanelItem, PanelItemUI>>,
}
impl PanelItemUIHandler {
    pub fn new(item_ui: ItemUI<PanelItem>) -> Self {
        PanelItemUIHandler {
            item_ui,
            items: FxHashMap::default(),
        }
    }
    pub fn frame(&mut self, info: &FrameInfo) {
        for (_, item) in self.items.iter() {
            item.lock_wrapped().frame(info, &self.item_ui);
        }
    }
}
impl ItemUIHandler<PanelItem> for PanelItemUIHandler {
    fn item_created(&mut self, uid: &str, item: PanelItem, init_data: PanelItemInitData) {
        let Ok(ui) = PanelItemUI::new(item.alias(), init_data) else {return};
        let Ok(ui) = item.wrap(ui) else {return};
        self.items.insert(uid.to_string(), ui);
    }
    fn item_captured(&mut self, uid: &str, acceptor_uid: &str, _item: PanelItem) {
        if let Some(ui) = self.items.get(uid) {
            ui.lock_wrapped().captured(acceptor_uid);
        }
    }
    fn item_released(&mut self, uid: &str, acceptor_uid: &str, _item: PanelItem) {
        if let Some(ui) = self.items.get(uid) {
            ui.lock_wrapped().released(acceptor_uid);
        }
    }
    fn item_destroyed(&mut self, uid: &str) {
        self.items.remove(uid);
    }
}

const PANEL_WIDTH: f32 = 0.1;
const PANEL_THICKNESS: f32 = 0.01;
const MAX_ACCEPT_DISTANCE: f32 = 0.05;
struct PanelItemUI {
    captured: bool,
    panel_item: PanelItem,
    model: Model,
    field: BoxField,
    grabbable: Grabbable,
    // update_position_task: JoinHandle<()>,
}
impl PanelItemUI {
    fn new(panel_item: PanelItem, init_data: PanelItemInitData) -> Result<Self, NodeError> {
        let field = BoxField::create(
            &panel_item,
            Transform::default(),
            [PANEL_WIDTH, PANEL_WIDTH, PANEL_THICKNESS].into(),
        )?;
        let grabbable = Grabbable::new(
            &panel_item,
            Transform::default(),
            &field,
            GrabData::default(),
        )?;
        let model = Model::create(
            grabbable.content_parent(),
            Transform::from_scale([PANEL_WIDTH, PANEL_WIDTH, PANEL_THICKNESS]),
            &ResourceID::new_namespaced("orbit", "panel"),
        )?;
        field.set_spatial_parent(grabbable.content_parent())?;
        panel_item.set_spatial_parent(grabbable.content_parent())?;

        let closest_acceptor_distance = Arc::new(Mutex::new((String::new(), f32::MAX)));
        let _closest_acceptor_distance = closest_acceptor_distance.clone();

        // let model_alias = model.alias();
        // let (get_distance_sender, mut get_distance_receiver) = mpsc::unbounded_channel();
        // let update_position_task = tokio::spawn(async move {
        //     while let Some(distances) = get_distance_receiver.recv().await {
        //         let mut closest_acceptor = None;
        //         let mut min_distance = f32::MAX;
        //         for (acceptor, distance) in distances {
        //             let Ok(distance) = distance.await else {continue};
        //             if distance < min_distance {
        //                 closest_acceptor.replace(acceptor);
        //             }
        //             min_distance = min_distance.min(distance);
        //         }
        //         let brightness = min_distance.recip();
        //         let _ = model_alias.set_material_parameter(
        //             1,
        //             "color",
        //             MaterialParameter::Color([brightness, brightness, brightness, 1.0]),
        //         );
        //         *_closest_acceptor_distance.lock().unwrap() =
        //             (closest_acceptor.unwrap_or_default(), min_distance);
        //     }
        // });
        let mut panel_item_ui = PanelItemUI {
            captured: false,
            panel_item,
            model,
            field,
            grabbable,
            // update_position_task,
        };
        panel_item_ui.commit_toplevel(init_data.toplevel);
        Ok(panel_item_ui)
    }
    fn captured(&mut self, _acceptor_uid: &str) {
        println!("Captured");
        self.captured = true;
    }
    fn released(&mut self, _acceptor_uid: &str) {
        println!("Released");
        self.captured = false;
    }
    fn frame(&mut self, info: &FrameInfo, item_ui: &ItemUI<PanelItem>) {
        self.grabbable.update(info);
        // When we start we want the item to move with the grabbable
        if self.grabbable.grab_action().actor_started() {
            let _ = self
                .grabbable
                .content_parent()
                .set_transform(Some(&self.panel_item), Transform::default());
            let _ = self
                .panel_item
                .set_spatial_parent_in_place(self.grabbable.content_parent());
        }
        if self.grabbable.grab_action().actor_stopped() {
            self.try_accept(item_ui);
        }
    }

    fn try_accept(&self, item_ui: &ItemUI<PanelItem>) {
        let acceptors = item_ui.acceptors();
        let keys = acceptors.keys().cloned().collect::<Vec<String>>();
        let fields = acceptors
            .values()
            .map(|(_, f)| f.alias())
            .collect::<Vec<_>>();
        drop(acceptors);

        let Ok(future) = self.grabbable
            .content_parent()
            .field_distance([0.0; 3], fields) else {return};
        let item_ui = item_ui.alias();
        let panel_item = self.panel_item.alias();
        tokio::spawn(async move {
            let Ok(distances) = future.await else {return};
            let Some((uid, distance)) = keys.into_iter()
                .zip(distances.into_iter().map(|d| d.map(|d| d.abs())))
                .filter_map(|(k, v)| Some((k, v?)))
                .reduce(|(ak, av), (bk, bv)| {
                    if av > bv {
                        (bk, bv)
                    } else {
                        (ak, av)
                    }
                }) else {return};

            if distance < MAX_ACCEPT_DISTANCE {
                let acceptors = item_ui.acceptors();
                let Some(acceptor) = acceptors.get(&uid).map(|(a, _)| a) else {return};
                let _ = acceptor.capture(&panel_item);
            }
        });
    }
}
impl PanelItemHandler for PanelItemUI {
    fn commit_toplevel(&mut self, state: Option<ToplevelInfo>) {
        dbg!(&state);
        let aspect_ratio = state
            .as_ref()
            .map(|t| t.size.y as f32 / t.size.x as f32)
            .unwrap_or(1.0);
        dbg!(&aspect_ratio);
        let size = [PANEL_WIDTH, PANEL_WIDTH * aspect_ratio, PANEL_THICKNESS];
        let _ = self.model.set_scale(None, size);
        let _ = self.field.set_size(size);
        let _ = self.panel_item.apply_toplevel_material(&self.model, 0);
    }
}
impl Drop for PanelItemUI {
    fn drop(&mut self) {
        // self.update_position_task.abort();
    }
}