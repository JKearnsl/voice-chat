fn voicechat_options(game: &mut crate::game::Game<'_>) {
    voice_chat::engine::set_language(&game.settings.language);
    crate::game::mods::voicechat_key_labels::translate(game);
}
