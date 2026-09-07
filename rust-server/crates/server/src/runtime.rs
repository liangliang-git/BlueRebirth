use super::*;
use crate::config::SharedPush;
use blueoath_domain::{AccountState, NewAccountFactory, ProfileId};
use blueoath_protocol::GameLoginCodec;
use blueoath_storage::ProfileStore;
use std::sync::OnceLock;

fn load_or_create_typed_account(
    store: &ProfileStore,
    profile_id: &str,
    name: &str,
) -> Result<AccountState, blueoath_storage::StorageError> {
    let profile_id = ProfileId::new(profile_id.to_owned())
        .map_err(|_| blueoath_storage::StorageError::InvalidProfileId)?;
    if let Some(account) = store.load_typed_account(&profile_id)? {
        return Ok(account);
    }
    let mut account = NewAccountFactory::create(profile_id, name.to_owned());
    store.save_typed_account(&mut account)?;
    Ok(account)
}

pub async fn run(config: ServerConfig) -> Result<(), ServerError> {
    let _ = BUILD_SHIP_CATALOG.get_or_init(|| load_build_ship_catalog(config.client_path.as_ref()));
    let _ = BUILD_FORMULA_CATALOG
        .get_or_init(|| load_build_formula_catalog(config.client_path.as_ref()));
    let _ = TALENT_CATALOG.get_or_init(|| load_talent_catalog(config.client_path.as_ref()));
    let _ = SHIP_STAT_CATALOG.get_or_init(|| load_ship_stat_catalog(config.client_path.as_ref()));
    let _ = HERO_SKILL_UPGRADE_CATALOG
        .get_or_init(|| load_hero_skill_upgrade_catalog(config.client_path.as_ref()));
    let _ = SHIP_INTENSIFY_CATALOG
        .get_or_init(|| load_ship_intensify_catalog(config.client_path.as_ref()));
    let _ = SHIP_BREAK_CATALOG.get_or_init(|| load_ship_break_catalog(config.client_path.as_ref()));
    let _ =
        SHIP_ADVANCE_CATALOG.get_or_init(|| load_ship_advance_catalog(config.client_path.as_ref()));
    let _ =
        SHIP_REMOULD_CATALOG.get_or_init(|| load_ship_remould_catalog(config.client_path.as_ref()));
    let _ = RECHARGE_CATALOG.get_or_init(|| load_recharge_catalog(config.client_path.as_ref()));
    let _ = GAMEPLAY_CATALOG.get_or_init(|| load_gameplay_catalog(config.client_path.as_ref()));
    let _ = COMMANDER_LEVEL_CATALOG
        .get_or_init(|| load_commander_level_catalog(config.client_path.as_ref()));
    let _ = SUPPORT_CATALOG.get_or_init(|| load_support_catalog(config.client_path.as_ref()));
    GAMEPLAY_CATALOG
        .get()
        .expect("gameplay catalog initialized")
        .validate()
        .map_err(ServerError::Catalog)?;
    SHIP_BREAK_CATALOG
        .get()
        .expect("ship break catalog initialized")
        .validate()
        .map_err(ServerError::Catalog)?;
    SHIP_ADVANCE_CATALOG
        .get()
        .expect("ship advance catalog initialized")
        .validate()
        .map_err(ServerError::Catalog)?;
    SHIP_REMOULD_CATALOG
        .get()
        .expect("ship remould catalog initialized")
        .validate()
        .map_err(ServerError::Catalog)?;
    let listener = TcpListener::bind(("127.0.0.1", config.port)).await?;
    let address = listener.local_addr()?;
    // Always expose game-login endpoint. Port 0 keeps default startup collision-free while
    // bootstrap responses advertise actual assigned port.
    let game_login_listener =
        if config.game_login_port.is_some() || config.kcp_game_login_port.is_none() {
            Some(TcpListener::bind(("127.0.0.1", config.game_login_port.unwrap_or(0))).await?)
        } else {
            None
        };
    let game_login_port = game_login_listener
        .as_ref()
        .map(TcpListener::local_addr)
        .transpose()?
        .map(|address| address.port());
    let kcp_listener = match config.kcp_game_login_port {
        Some(port) => Some(Arc::new(UdpSocket::bind(("127.0.0.1", port)).await?)),
        None => None,
    };
    let kcp_game_login_port = kcp_listener
        .as_ref()
        .map(|socket| socket.local_addr())
        .transpose()?
        .map(|address| address.port());
    let advertised_game_login_port = game_login_port.or(kcp_game_login_port);
    let store = ProfileStore::open(&config.data_root)?;
    let mut shop = load_shop_catalog(config.client_path.as_ref());
    load_server_shop_goods(&mut shop, &config.data_root);
    let catalogs = Arc::new(GameCatalogs {
        chapters: Arc::new(load_chapter_catalog(config.client_path.as_ref())),
        fashion: Arc::new(load_fashion_catalog(config.client_path.as_ref())),
        equip: Arc::new(load_equip_catalog(config.client_path.as_ref())),
        shop: Arc::new(shop),
        handbook_behaviours: Arc::new(load_handbook_behaviours(config.client_path.as_ref())),
        hero_memories: Arc::new(load_hero_memories(config.client_path.as_ref())),
        tasks: Arc::new(load_task_catalog(config.client_path.as_ref())),
        battle: Arc::new(load_battle_catalog(config.client_path.as_ref())),
        mails: Arc::new(load_server_mail_templates(&config.data_root)),
        hero_level: Arc::new(load_hero_level_catalog(config.client_path.as_ref())),
        hero_breakdown: Arc::new(load_hero_breakdown_catalog(config.client_path.as_ref())),
        buildings: Arc::new(load_building_catalog(config.client_path.as_ref())),
        equip_new_test: Arc::new(load_equip_new_test_catalog(config.client_path.as_ref())),
        affection: Arc::new(load_affection_catalog(config.client_path.as_ref())),
        combination: Arc::new(load_combination_catalog(config.client_path.as_ref())),
    });
    catalogs.validate().map_err(ServerError::Catalog)?;
    let profile_id = config.profile_id.clone();
    let profile_name = config.profile_name.clone();
    let version = config.version.clone();
    let typed_account = load_or_create_typed_account(&store, &profile_id, &profile_name)?;
    let initial_name = typed_account
        .profile
        .as_ref()
        .map(|profile| profile.name.clone())
        .unwrap_or(profile_name);
    let mut initial_state = ServerState::new(profile_id.clone(), initial_name, version);
    initial_state.battle_port = address.port();
    // Runtime tuning is server-owned; persisted player snapshots keep only gameplay state.
    initial_state.drop_multiplier = normalize_multiplier(config.drop_multiplier);
    initial_state.ship_exp_multiplier = normalize_multiplier(config.ship_exp_multiplier);
    initial_state.commander_exp_multiplier = normalize_multiplier(config.commander_exp_multiplier);
    initial_state.ship_stat_multiplier = normalize_multiplier(config.ship_stat_multiplier);
    initial_state.mood_recovery_multiplier = normalize_multiplier(config.mood_recovery_multiplier);
    initial_state.affection_multiplier = normalize_multiplier(config.affection_multiplier);
    initial_state.building_oil_multiplier = normalize_multiplier(config.building_oil_multiplier);
    initial_state.building_gold_multiplier = normalize_multiplier(config.building_gold_multiplier);
    initial_state.social_store = Some(store.clone());
    let state = Arc::new(Mutex::new(initial_state));
    let persist_lock = Arc::new(tokio::sync::Mutex::new(()));
    println!(
        "{}",
        serde_json::to_string(&json!({
            "ready": true,
            "port": address.port(),
            "gameLoginPort": advertised_game_login_port,
            "kcpGameLoginPort": kcp_game_login_port,
            "profileId": profile_id,
        }))?
    );

    if let Some(listener) = game_login_listener {
        let state = Arc::clone(&state);
        let store = store.clone();
        let persist_lock = Arc::clone(&persist_lock);
        let catalogs = Arc::clone(&catalogs);
        tokio::spawn(async move {
            if let Err(error) =
                run_game_login_listener(listener, state, store, persist_lock, catalogs).await
            {
                eprintln!("game-login listener failed: {error}");
            }
        });
    }

    if let Some(listener) = kcp_listener {
        let state = Arc::clone(&state);
        let store = store.clone();
        let persist_lock = Arc::clone(&persist_lock);
        let catalogs = Arc::clone(&catalogs);
        tokio::spawn(async move {
            if let Err(error) =
                run_kcp_game_login_listener(listener, state, store, persist_lock, catalogs).await
            {
                eprintln!("kcp-game-login listener failed: {error}");
            }
        });
    }

    loop {
        let (stream, _) = listener.accept().await?;
        let state = Arc::clone(&state);
        let store = store.clone();
        let persist_lock = Arc::clone(&persist_lock);
        let catalogs = Arc::clone(&catalogs);
        tokio::spawn(async move {
            if let Err(error) = handle_connection(
                stream,
                state,
                store,
                persist_lock,
                advertised_game_login_port,
                catalogs,
            )
            .await
            {
                eprintln!("connection failed: {error}");
            }
        });
    }
}

async fn run_game_login_listener(
    listener: TcpListener,
    state: Arc<Mutex<ServerState>>,
    store: ProfileStore,
    persist_lock: Arc<tokio::sync::Mutex<()>>,
    catalogs: Arc<GameCatalogs>,
) -> Result<(), ServerError> {
    loop {
        let (stream, _) = listener.accept().await?;
        let state = Arc::clone(&state);
        let store = store.clone();
        let persist_lock = Arc::clone(&persist_lock);
        let catalogs = Arc::clone(&catalogs);
        tokio::spawn(async move {
            if let Err(error) =
                handle_game_login_connection(stream, state, store, persist_lock, catalogs).await
            {
                eprintln!("game-login connection failed: {error}");
            }
        });
    }
}

/// In-memory transport keeps persistence lock away from untrusted socket I/O.
struct BufferedNetSocket {
    input: Vec<u8>,
    input_offset: usize,
    output: Vec<u8>,
}

impl BufferedNetSocket {
    fn from_frame(frame: &blueoath_transport::NetSocketFrame) -> Self {
        let mut input = Vec::with_capacity(5 + frame.payload.len() + 16);
        input.extend_from_slice(&(frame.payload.len() as i32).to_be_bytes());
        input.push(frame.frame_type);
        if frame.frame_type == 1 {
            input.extend_from_slice(&[0_u8; 16]);
        }
        input.extend_from_slice(&frame.payload);
        Self {
            input,
            input_offset: 0,
            output: Vec::new(),
        }
    }

    fn into_output(self) -> Vec<u8> {
        self.output
    }
}

impl AsyncRead for BufferedNetSocket {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.input_offset >= self.input.len() {
            return Poll::Ready(Ok(()));
        }
        let remaining = &self.input[self.input_offset..];
        let copied = remaining.len().min(buf.remaining());
        buf.put_slice(&remaining[..copied]);
        self.input_offset += copied;
        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for BufferedNetSocket {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.output.extend_from_slice(bytes);
        Poll::Ready(Ok(bytes.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

async fn handle_game_login_connection<S>(
    stream: S,
    state: Arc<Mutex<ServerState>>,
    store: ProfileStore,
    persist_lock: Arc<tokio::sync::Mutex<()>>,
    catalogs: Arc<GameCatalogs>,
) -> Result<(), ServerError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (mut reader, mut writer) = tokio::io::split(stream);
    let profile_id = {
        let state = state
            .lock()
            .map_err(|_| ServerError::InvalidMessage("state mutex poisoned".to_owned()))?;
        state.profile_id.clone()
    };
    let connection_uid = blueoath_domain::ProfileId::new(profile_id.clone())
        .ok()
        .and_then(|id| store.load_typed_account(&id).ok().flatten())
        .map(|account| account.character.uid)
        .unwrap_or(1);
    let mut shared_events = {
        let state = state
            .lock()
            .map_err(|_| ServerError::InvalidMessage("state mutex poisoned".to_owned()))?;
        let shared = state
            .shared_social
            .lock()
            .map_err(|_| ServerError::InvalidMessage("shared social state poisoned".to_owned()))?;
        shared.push_tx.clone().subscribe()
    };
    loop {
        tokio::select! {
            frame = NetSocketFrameCodec::read(&mut reader) => {
                let Some(frame) = frame? else {
                    return Ok(());
                };
                let frame_context = ConnectionFrameContext {
                    state: &state,
                    store: &store,
                    persist_lock: &persist_lock,
                    catalogs: &catalogs,
                    profile_id: &profile_id,
                };
                let keep_alive =
                    process_connection_frame(&mut writer, &frame_context, frame).await?;
                if !keep_alive {
                    return Ok(());
                }
            }
            event = shared_events.recv() => {
                match event {
                    Ok(event) if event.recipient_uid == connection_uid => {
                        acknowledge_shared_push(&state, &event);
                        let response = TMessageCodec::encode_response(&TResponse {
                            method: event.method,
                            ret: Some(event.payload),
                            time: current_unix_seconds(),
                            ..TResponse::default()
                        });
                        NetSocketFrameCodec::write(&mut writer, 0, &response).await?;
                        writer.flush().await?;
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        // Request path still drains persisted pending pushes.
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {}
                }
            }
        }
    }
}

struct ConnectionFrameContext<'a> {
    state: &'a Arc<Mutex<ServerState>>,
    store: &'a ProfileStore,
    persist_lock: &'a Arc<tokio::sync::Mutex<()>>,
    catalogs: &'a Arc<GameCatalogs>,
    profile_id: &'a str,
}

async fn process_connection_frame<S>(
    stream: &mut S,
    context: &ConnectionFrameContext<'_>,
    frame: blueoath_transport::NetSocketFrame,
) -> Result<bool, ServerError>
where
    S: AsyncWrite + Unpin,
{
    // Process against an in-memory transport while holding the profile lock;
    // only buffered response bytes are written to the untrusted socket after
    // the account transaction commits.
    let _persist_guard = context.persist_lock.lock().await;
    let mut typed_account = {
        let account_profile_id = context.profile_id.to_owned();
        let account_store = context.store.clone();
        tokio::task::spawn_blocking(move || {
            load_or_create_typed_account(&account_store, &account_profile_id, &account_profile_id)
        })
        .await
        .map_err(|error| ServerError::StorageTask(error.to_string()))??
    };
    let state_snapshot = {
        let state = context
            .state
            .lock()
            .map_err(|_| ServerError::InvalidMessage("state mutex poisoned".to_owned()))?;
        state.clone()
    };
    let typed_account_before = typed_account.clone();
    let mut buffered = BufferedNetSocket::from_frame(&frame);
    let login_catalogs = context.catalogs.login_catalogs();
    let keep_alive = process_game_login_frame_payload_with_typed_account(
        &mut buffered,
        &state_snapshot,
        &mut typed_account,
        frame,
        &login_catalogs,
    )
    .await?;
    if typed_account != typed_account_before {
        let account_store = context.store.clone();
        tokio::task::spawn_blocking(move || account_store.save_typed_account(&mut typed_account))
            .await
            .map_err(|error| ServerError::StorageTask(error.to_string()))??;
    }
    let responses = buffered.into_output();
    drop(_persist_guard);
    stream.write_all(&responses).await?;
    stream.flush().await?;
    Ok(keep_alive)
}

fn acknowledge_shared_push(state: &Arc<Mutex<ServerState>>, event: &SharedPush) {
    let Ok(state) = state.lock() else {
        return;
    };
    let Ok(mut shared) = state.shared_social.lock() else {
        return;
    };
    let Some(pending) = shared.pending_pushes.get_mut(&event.recipient_uid) else {
        return;
    };
    if let Some(index) = pending
        .iter()
        .position(|(method, payload)| method == &event.method && payload == &event.payload)
    {
        pending.remove(index);
    }
    if pending.is_empty() {
        shared.pending_pushes.remove(&event.recipient_uid);
    }
}

struct KcpPeer {
    connection: KcpConnection,
    profile_id: String,
    uid: u64,
    last_seen: u32,
    shared_events: tokio::sync::broadcast::Receiver<SharedPush>,
}

const KCP_MAX_PEERS: usize = 1024;
const KCP_PEER_IDLE_MS: u32 = 120_000;

async fn run_kcp_game_login_listener(
    socket: Arc<UdpSocket>,
    state: Arc<Mutex<ServerState>>,
    store: ProfileStore,
    persist_lock: Arc<tokio::sync::Mutex<()>>,
    catalogs: Arc<GameCatalogs>,
) -> Result<(), ServerError> {
    let mut peers = std::collections::HashMap::<std::net::SocketAddr, KcpPeer>::new();
    let mut buffer = vec![0_u8; 65_535];
    loop {
        tokio::select! {
            received = socket.recv_from(&mut buffer) => {
                let (length, endpoint) = received?;
                let trace_kcp = std::env::var_os("BLUEOATH_TRACE_KCP").is_some();
                if trace_kcp {
                    eprintln!("kcp datagram rx endpoint={endpoint} bytes={length}");
                }
                let now = current_unix_millis();
                let mut peer = if let Some(mut peer) = peers.remove(&endpoint) {
                    peer.last_seen = now;
                    peer
                } else {
                    if peers.len() >= KCP_MAX_PEERS { continue; }
                    KcpPeer {
                        connection: KcpConnection::new(0),
                        profile_id: state.lock().map(|state| state.profile_id.clone()).unwrap_or_else(|_| DEFAULT_PROFILE_ID.to_owned()),
                        uid: 0,
                        last_seen: now,
                        shared_events: shared_event_receiver(&state)?,
                    }
                };
                let mut offset = 0;
                while offset < length {
                    let decoded = match KcpPacket::decode(&buffer[offset..length]) {
                        Ok(decoded) => decoded,
                        Err(_) => break,
                    };
                    let Some((packet, consumed)) = decoded else { break };
                    offset += consumed;
                    if trace_kcp {
                        eprintln!(
                            "kcp packet endpoint={endpoint} conv={} command={:?} frg={} sn={} una={} data={}",
                            packet.conv,
                            packet.command,
                            packet.fragment,
                            packet.sequence_number,
                            packet.unacknowledged,
                            packet.data.len()
                        );
                    }
                    if peer.connection.conv() != packet.conv {
                        let profile_id = peer.profile_id.clone();
                        let uid = peer.uid;
                        peer = KcpPeer {
                            connection: KcpConnection::new(packet.conv),
                            profile_id,
                            uid,
                            last_seen: now,
                            shared_events: shared_event_receiver(&state)?,
                        };
                    }
                    let messages = peer.connection.input(packet);
                    for message in messages {
                        if trace_kcp {
                            eprintln!(
                                "kcp application message endpoint={endpoint} bytes={} prefix={}",
                                message.len(),
                                message
                                    .iter()
                                    .take(24)
                                    .map(|byte| format!("{byte:02x}"))
                                    .collect::<String>()
                            );
                        }
                        let responses = match build_kcp_wire_responses(
                            message,
                            &mut peer,
                            &state,
                            &store,
                            &persist_lock,
                            &catalogs,
                        ).await {
                            Ok(responses) => responses,
                            Err(error) => {
                                eprintln!("dropping invalid KCP application message from {endpoint}: {error}");
                                continue;
                            }
                        };
                        for response in responses {
                            if let Err(error) = peer.connection.send(&response, now) {
                                eprintln!("dropping oversized KCP response for {endpoint}: {error}");
                            }
                        }
                    }
                }
                drain_kcp_shared_events(&mut peer, &state, now)?;
                for datagram in peer.connection.flush(now) {
                    if trace_kcp {
                        eprintln!("kcp datagram tx endpoint={endpoint} bytes={}", datagram.len());
                    }
                    socket.send_to(&datagram, endpoint).await?;
                }
                if !peer.connection.dead() { peers.insert(endpoint, peer); }
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(50)) => {
                let now = current_unix_millis();
                let mut dead = Vec::new();
                for (endpoint, peer) in &mut peers {
                    drain_kcp_shared_events(peer, &state, now)?;
                    for datagram in peer.connection.flush(now) {
                        socket.send_to(&datagram, *endpoint).await?;
                    }
                    if peer.connection.dead() || now.wrapping_sub(peer.last_seen) > KCP_PEER_IDLE_MS {
                        dead.push(*endpoint);
                    }
                }
                for endpoint in dead { peers.remove(&endpoint); }
            }
        }
    }
}

fn shared_event_receiver(
    state: &Arc<Mutex<ServerState>>,
) -> Result<tokio::sync::broadcast::Receiver<SharedPush>, ServerError> {
    let state = state
        .lock()
        .map_err(|_| ServerError::InvalidMessage("state mutex poisoned".to_owned()))?;
    let shared = state
        .shared_social
        .lock()
        .map_err(|_| ServerError::InvalidMessage("shared social state poisoned".to_owned()))?;
    Ok(shared.push_tx.subscribe())
}

fn drain_kcp_shared_events(
    peer: &mut KcpPeer,
    state: &Arc<Mutex<ServerState>>,
    now: u32,
) -> Result<(), ServerError> {
    loop {
        let event = match peer.shared_events.try_recv() {
            Ok(event) => event,
            Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::TryRecvError::Closed) => break,
        };
        if event.recipient_uid != peer.uid {
            continue;
        }
        acknowledge_shared_push(state, &event);
        let response = TMessageCodec::encode_response(&TResponse {
            method: event.method,
            ret: Some(event.payload),
            time: current_unix_seconds(),
            ..TResponse::default()
        });
        let wire = ClientGameWireCodec::encode_server_response(6, &response);
        peer.connection.send(&wire, now).map_err(|error| {
            ServerError::InvalidMessage(format!("KCP shared push failed: {error}"))
        })?;
    }
    Ok(())
}

async fn build_kcp_wire_responses(
    message: Vec<u8>,
    peer: &mut KcpPeer,
    state: &Arc<Mutex<ServerState>>,
    store: &ProfileStore,
    persist_lock: &Arc<tokio::sync::Mutex<()>>,
    catalogs: &Arc<GameCatalogs>,
) -> Result<Vec<Vec<u8>>, ServerError> {
    let message = ClientGameWireCodec::decode_client_request(&message)?;
    if message.channel != 0 {
        return Ok(Vec::new());
    }
    if message.operation == 2 {
        let login = GameLoginCodec::decode_login(&message.payload)?;
        peer.profile_id = normalize_profile_id(&login.pid);
        let profile_id = peer.profile_id.clone();
        let account_store = store.clone();
        let _guard = persist_lock.lock().await;
        tokio::task::spawn_blocking(move || {
            let _ = load_or_create_typed_account(&account_store, &profile_id, &profile_id)?;
            Ok::<_, blueoath_storage::StorageError>(())
        })
        .await
        .map_err(|error| ServerError::StorageTask(error.to_string()))??;
        let response = GameLoginCodec::encode_response(&TRetLogin {
            ret: "ok".to_owned(),
            feign_role_id: peer.profile_id.clone(),
            err_code: 0,
        });
        return Ok(vec![ClientGameWireCodec::encode_server_response(
            2, &response,
        )]);
    }
    if message.operation != 5 {
        return Ok(Vec::new());
    }

    let _persist_guard = persist_lock.lock().await;
    let profile_id = peer.profile_id.clone();
    let account_store = store.clone();
    let mut typed_account = tokio::task::spawn_blocking(move || {
        load_or_create_typed_account(&account_store, &profile_id, &profile_id)
    })
    .await
    .map_err(|error| ServerError::StorageTask(error.to_string()))??;
    peer.uid = blueoath_domain::ProfileId::new(peer.profile_id.clone())
        .ok()
        .and_then(|id| store.load_typed_account(&id).ok().flatten())
        .map(|account| account.character.uid)
        .unwrap_or(1);
    let state_snapshot = state
        .lock()
        .map_err(|_| ServerError::InvalidMessage("state mutex poisoned".to_owned()))?
        .clone();
    let (mut input, mut output) = tokio::io::duplex(16 * 1024 * 1024);
    NetSocketFrameCodec::write(&mut input, 0, &message.payload).await?;
    input.shutdown().await?;
    let login_catalogs = catalogs.login_catalogs();
    process_game_login_frame_with_typed_account_and_catalogs(
        &mut output,
        &state_snapshot,
        &mut typed_account,
        &login_catalogs,
    )
    .await?;
    let account_store = store.clone();
    tokio::task::spawn_blocking(move || account_store.save_typed_account(&mut typed_account))
        .await
        .map_err(|error| ServerError::StorageTask(error.to_string()))??;
    let mut responses = Vec::new();
    loop {
        let frame = tokio::time::timeout(
            std::time::Duration::from_millis(20),
            NetSocketFrameCodec::read(&mut input),
        )
        .await;
        let Ok(frame) = frame else { break };
        let Some(frame) = frame? else { break };
        if frame.frame_type == 0 {
            responses.push(ClientGameWireCodec::encode_server_response(
                6,
                &frame.payload,
            ));
        }
    }
    Ok(responses)
}

static KCP_CLOCK: OnceLock<Instant> = OnceLock::new();

pub(super) fn current_unix_millis() -> u32 {
    KCP_CLOCK.get_or_init(Instant::now).elapsed().as_millis() as u32
}

pub(super) fn normalize_task_state(account: &mut Value, now: u32) -> bool {
    let day = (i64::from(now) + 8 * 60 * 60) / 86_400;
    let week = (day + 3) / 7;
    let Some(root) = account.as_object_mut() else {
        return false;
    };
    let tasks = root
        .entry("tasks")
        .or_insert_with(|| json!({}))
        .as_object_mut();
    let Some(tasks) = tasks else { return false };
    let old_day = value_i64_any_object(tasks, &["dailyResetDay", "daily_reset_day"]);
    let old_week = value_i64_any_object(tasks, &["weeklyResetWeek", "weekly_reset_week"]);
    let mut records = tasks
        .get("records")
        .or_else(|| tasks.get("Records"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut changed = !tasks.contains_key("records") || old_day != day || old_week != week;
    if old_day > 0 && old_day != day {
        let before = records.len();
        records
            .retain(|record| !matches!(value_i64_any(record, &["taskType", "task_type"]), 2 | 8));
        changed |= before != records.len();
    }
    if old_week > 0 && old_week != week {
        let before = records.len();
        records.retain(|record| value_i64_any(record, &["taskType", "task_type"]) != 3);
        changed |= before != records.len();
    }
    if changed {
        tasks.insert("records".to_owned(), Value::Array(records));
        tasks.insert("dailyResetDay".to_owned(), json!(day));
        tasks.insert("weeklyResetWeek".to_owned(), json!(week));
        if !tasks.contains_key("teachingPtRewardIds") {
            tasks.insert("teachingPtRewardIds".to_owned(), Value::Array(Vec::new()));
        }
    }
    changed
}

fn value_i64_any_object(object: &serde_json::Map<String, Value>, keys: &[&str]) -> i64 {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(Value::as_i64))
        .unwrap_or_default()
}

async fn handle_connection(
    mut stream: TcpStream,
    state: Arc<Mutex<ServerState>>,
    store: ProfileStore,
    persist_lock: Arc<tokio::sync::Mutex<()>>,
    game_login_port: Option<u16>,
    catalogs: Arc<GameCatalogs>,
) -> Result<(), ServerError> {
    let Some(prefix) = read_connection_prefix(&mut stream).await? else {
        return Ok(());
    };
    if looks_like_http(&prefix) {
        return handle_bootstrap_http(stream, prefix, state, game_login_port).await;
    }
    if looks_like_netsocket(&prefix) {
        let stream = PrefixedTcpStream::new(prefix, stream);
        return handle_game_login_connection(stream, state, store, persist_lock, catalogs).await;
    }
    let mut stream = PrefixedTcpStream::new(prefix, stream);
    loop {
        let Some(bytes) = FrameCodec::read(&mut stream).await? else {
            break;
        };
        let response = {
            let _persist_guard = persist_lock.lock().await;
            let (response, candidate) = {
                let state = state
                    .lock()
                    .map_err(|_| ServerError::InvalidMessage("state mutex poisoned".to_owned()))?;
                prepare_local_request(&state, &bytes)?
            };
            let store_task = store.clone();
            let profile_id = candidate.profile_id.clone();
            let profile_name = candidate.name.clone();
            let stored_state = candidate.stored_profile_state();
            let save_result = tokio::task::spawn_blocking(move || {
                store_task.save(&profile_id, &profile_name, &stored_state)
            })
            .await
            .map_err(|error| ServerError::StorageTask(error.to_string()))?
            .map_err(ServerError::from);
            match save_result {
                Ok(()) => {
                    let mut state = state.lock().map_err(|_| {
                        ServerError::InvalidMessage("state mutex poisoned".to_owned())
                    })?;
                    *state = candidate;
                    response
                }
                Err(error) => storage_failure_response(&bytes, &error)?,
            }
        };
        FrameCodec::write(&mut stream, &response).await?;
    }
    Ok(())
}
