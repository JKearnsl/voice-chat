// Releasing the pointer finishes the threshold drag and saves it once.
fn voicechat_mouse_release(game: &crate::game::Game<'_>, button: i32, canceled: &mut bool) {
    use voicechat_engine as vc;
    if game.native_mod_screen() != Some(vc::ID) {
        return;
    }
    *canceled = true;
    if button == 0 {
        vc::menu_release();
        for (k, v) in vc::take_store() {
            game.native_set_setting(vc::ID, &k, &v);
        }
    }
}
