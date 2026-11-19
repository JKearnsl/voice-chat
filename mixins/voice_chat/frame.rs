// Off a server (the title screen, single player, a lost connection): the
// voice chat hangs up. The menu also follows closes requested by either side.
fn voicechat_frame(game: &mut crate::game::Game<'_>) {
    use voicechat_engine as vc;
    // The player's language, for the menu, HUD and chat. Set once a frame on
    // the game thread; every other hook this frame reads the same value.
    vc::set_language(&game.settings.language);
    let on_server = game.remote.as_ref().is_some_and(|r| r.joined);
    if !on_server || !game.native_on(vc::ID) {
        vc::tick(None, &|_| None);
    }
    if vc::menu_take_close() {
        game.native_mod_screen_close(vc::ID);
    }
    vc::menu_sync(game.native_mod_screen() == Some(vc::ID));
    if vc::take_ui_click() {
        game.audio
            .play("minecraft:ui.button.click", None, 0.25, 1.0);
    }
}
