use crate::{config, engine as vc};
use mod_api::{
    ClientHost, ConfigKind, Flow, Hooks, KeyRegistration, Registrar, RegistrationContext,
};

#[allow(dead_code)]
#[derive(Default)]
pub struct VoiceChat;

#[mod_api::horizon_mod]
impl Hooks for VoiceChat {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        for (name, what, default) in vc::HOTKEYS {
            r.register_key_mapping(KeyRegistration::new(name, default).label(what));
        }
    }

    fn register_configs(
        &self,
        _: mod_api::HostSide,
        event: &mut mod_api::RegisterConfigsEvent<'_>,
    ) {
        event.configs.push(config::declaration());
    }

    fn config_loading(&self, _: mod_api::HostSide, event: &mut mod_api::ConfigLoadingEvent<'_>) {
        if event.config.mod_id == vc::ID && event.config.kind == ConfigKind::Client {
            config::load(&event.config.text);
        }
    }

    fn config_reloading(
        &self,
        _: mod_api::HostSide,
        event: &mut mod_api::ConfigReloadingEvent<'_>,
    ) {
        if event.config.mod_id == vc::ID && event.config.kind == ConfigKind::Client {
            config::reload(&event.config.text);
        }
    }

    fn client_setup(&self, host: &mut dyn ClientHost, _: &mut mod_api::ClientSetupEvent) {
        if let Some(language) = host.client_session().client_language() {
            vc::set_language(&language);
        }
    }

    fn client_tick_pre(&self, host: &mut dyn ClientHost, _: &mut mod_api::ClientTickPreEvent) {
        if let Some(language) = host.client_session().client_language() {
            vc::set_language(&language);
        }
        if !host.on(vc::ID) || host.client_session().client_server().is_none() {
            vc::tick(None, &|_| None);
            config::save(host);
            return;
        }

        vc::key(vc::Key::PushToTalk, host.is_down(vc::ID, vc::KEY_PTT));
        vc::key(vc::Key::Whisper, host.is_down(vc::ID, vc::KEY_WHISPER));
        if host.consume_click(vc::ID, vc::KEY_MUTE) {
            vc::key(vc::Key::Mute, true);
        }
        if host.consume_click(vc::ID, vc::KEY_DISABLE) {
            vc::key(vc::Key::Disable, true);
        }
        if host.consume_click(vc::ID, vc::KEY_MENU) {
            let key = host
                .mod_key(vc::ID, vc::KEY_MENU)
                .map(|(key, _)| key)
                .unwrap_or_else(|| vc::DEFAULT_MENU.to_owned());
            vc::menu_open(vc::key_char(&key));
            if !host.mod_screen_open(vc::ID) {
                vc::menu_sync(false);
            }
        }
        if vc::menu_take_close() {
            host.mod_screen_close(vc::ID);
        }
        vc::menu_sync(host.mod_screen() == Some(vc::ID));
        if vc::take_ui_click() {
            host.play_local_sound(&mod_api::SoundPlay::ui("minecraft:ui.button.click"), 0);
        }
        config::save(host);
    }

    fn client_disconnecting(
        &self,
        host: &dyn ClientHost,
        _: &mut mod_api::ClientDisconnectingEvent,
    ) {
        vc::tick(None, &|_| None);
        config::save(host);
    }

    fn client_stopping(&self, host: &mut dyn ClientHost, _: &mut mod_api::ClientStoppingEvent) {
        vc::tick(None, &|_| None);
        config::save(host);
    }

    fn client_packet(&self, _: &mut dyn ClientHost, event: &mut mod_api::ClientPacketEvent<'_>) {
        if let Some(payload) = &event.packet.custom_payload {
            vc::payload(&payload.channel, &payload.data);
        }
    }

    fn screen_render_post(
        &self,
        host: &dyn ClientHost,
        event: &mut mod_api::ScreenRenderPostEvent<'_>,
    ) {
        if host.mod_screen() != Some(vc::ID) {
            return;
        }
        let hotkeys = vc::HOTKEYS
            .iter()
            .map(|(name, what, default)| {
                let key = host
                    .mod_key(vc::ID, name)
                    .map(|(key, _)| key)
                    .unwrap_or_else(|| default.to_string());
                (what.to_string(), vc::key_label(&key))
            })
            .collect();
        let size = event.draw.size();
        let mouse = event.screen.mouse;
        let commands = {
            let measure = |text: &str| event.draw.text_width(text);
            vc::menu_draw(size, mouse, &measure, hotkeys)
        };
        for command in commands {
            match command {
                vc::ui::Draw::Fill(rect, argb) => event.draw.fill(rect, argb),
                vc::ui::Draw::Text(text, at, rgb, shadow) => {
                    event.draw.text(&text, at, rgb, shadow);
                }
                vc::ui::Draw::Sprite { name, src, dst } => {
                    event.draw.sprite(name, src, dst, None);
                }
            }
        }
    }

    fn screen_mouse_button_pressed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut mod_api::ScreenMouseButtonPressedPreEvent<'_>,
    ) -> Flow {
        if host.mod_screen() != Some(vc::ID) {
            return Flow::Continue;
        }
        if event.button == 0 {
            vc::menu_click(event.screen.mouse);
            config::save(host);
        }
        Flow::Cancel
    }

    fn screen_mouse_button_released_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut mod_api::ScreenMouseButtonReleasedPreEvent<'_>,
    ) -> Flow {
        if host.mod_screen() != Some(vc::ID) {
            return Flow::Continue;
        }
        if event.button == 0 {
            vc::menu_release();
            config::save(host);
        }
        Flow::Cancel
    }

    fn screen_mouse_dragged_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut mod_api::ScreenMouseDraggedPreEvent<'_>,
    ) -> Flow {
        if host.mod_screen() != Some(vc::ID) {
            return Flow::Continue;
        }
        if event.button == 0 {
            vc::menu_drag(event.screen.mouse);
        }
        Flow::Cancel
    }

    fn screen_mouse_scrolled_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut mod_api::ScreenMouseScrolledPreEvent<'_>,
    ) -> Flow {
        if host.mod_screen() != Some(vc::ID) {
            return Flow::Continue;
        }
        vc::menu_scroll(event.scroll[1]);
        Flow::Cancel
    }

    fn screen_key_pressed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut mod_api::ScreenKeyPressedPreEvent<'_>,
    ) -> Flow {
        if host.mod_screen() != Some(vc::ID) {
            return Flow::Continue;
        }
        let menu = host
            .mod_key(vc::ID, vc::KEY_MENU)
            .map(|(key, _)| key)
            .unwrap_or_else(|| vc::DEFAULT_MENU.to_owned());
        vc::menu_key(event.key, event.modifiers & 1 != 0, &menu);
        config::save(host);
        Flow::Cancel
    }

    fn screen_char_typed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut mod_api::ScreenCharTypedPreEvent<'_>,
    ) -> Flow {
        if host.mod_screen() != Some(vc::ID) {
            return Flow::Continue;
        }
        vc::menu_char(event.c);
        Flow::Cancel
    }
}
