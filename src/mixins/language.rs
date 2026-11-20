fn voicechat_language(game: &mut crate::game::Game<'_>, code: &str) {
    voice_chat::engine::set_language(code);
    crate::game::mods::voicechat_key_labels::translate(game);
}
