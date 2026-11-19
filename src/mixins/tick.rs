fn voicechat_tick(game: &mut crate::game::Game<'_>) {
    use voice_chat::engine as vc;
    let Some(r) = game.remote.as_ref() else {
        return;
    };
    if !game.native_on(vc::ID) || !r.joined || r.reconfiguring.is_some() {
        vc::tick(None, &|_| None);
        return;
    }
    let address = r.session.address.clone();
    let players: Vec<(u128, [f64; 3])> = r
        .level_players
        .iter()
        .filter_map(|(&id, &uuid)| {
            let [x, y, z] = game.remote_entity_position(id)?;
            Some((uuid, [x, y + vc::EYE_HEIGHT, z]))
        })
        .collect();
    let listener = game.native_player().map(|p| vc::mix::Listener {
        eye: [p.x, p.y + vc::EYE_HEIGHT, p.z],
        yaw: p.yaw,
    });
    let world = vc::World {
        address: &address,
        listener,
        players,
        redirect: horizon_protocol::server_redirect,
    };
    let done = vc::tick(Some(world), &voice_chat::config::get);
    for (channel, data) in done.payloads {
        game.send(horizon_protocol::packets::play::Serverbound::CustomPayload(
            horizon_protocol::packets::configuration::CustomPayload { channel, data },
        ));
    }
    for line in done.chat {
        game.native_chat(&line);
    }
}
