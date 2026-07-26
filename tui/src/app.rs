//! 应用运行时及异步副作用。

use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyEventKind};
use futures::StreamExt;
use sculk::persist::Profile;
use sculk::tunnel::{
    HostConfig, HostOptions, JoinConfig, JoinOptions, Ticket, TunnelMode, TunnelPhase,
    TunnelService, TunnelUpdate,
};
use tokio::task::JoinSet;
use tokio::time;

use crate::model::{ActiveTab, Intent, Model, RELAYS};
use crate::terminal::TerminalSession;
use crate::ui;

const TICK: Duration = Duration::from_millis(200);

/// 运行终端事件循环，直到用户退出。
pub async fn run_tui() -> anyhow::Result<()> {
    let mut terminal = TerminalSession::enter()?;

    let (mut profile, profile_err) = load_profile();
    let service = TunnelService::new();
    let mut tunnel_updates = service.subscribe();
    let mut model = Model::new(&profile, service.status(), profile_err);
    let mut event_stream = EventStream::new();
    let mut tick_interval = time::interval(TICK);
    let mut commands = JoinSet::new();

    loop {
        terminal.draw(|frame| ui::render(frame, &mut model))?;

        tokio::select! {
            maybe_event = event_stream.next() => {
                let Some(event_result) = maybe_event else { break };
                let event = match event_result {
                    Ok(event) => event,
                    Err(e) => {
                        model.add_log(&format!("事件读取失败: {e}"));
                        continue;
                    }
                };
                let Event::Key(key) = event else { continue };
                if key.kind == KeyEventKind::Release { continue }
                let intent = crate::keymap::handle_key(&mut model, key);
                if matches!(intent, Intent::Exit) { break }
                handle_intent(intent, &mut model, &mut profile, &service, &mut commands).await;
            }
            update = tunnel_updates.recv() => {
                let Some(update) = update else { break };
                copy_host_ticket(&mut model, &update);
                remember_join_ticket(&mut model, &mut profile, &update);
                model.handle_tunnel_update(update);
            }
            _ = tick_interval.tick() => {
                model.on_tick();
            }
            result = commands.join_next(), if !commands.is_empty() => {
                match result {
                    Some(Ok(Some(message))) => model.add_log(&message),
                    Some(Err(e)) => model.add_log(&format!("后台任务失败: {e}")),
                    Some(Ok(None)) | None => {}
                }
            }
        }
    }

    commands.abort_all();
    let _ = tokio::time::timeout(Duration::from_secs(3), service.shutdown()).await;

    Ok(())
}

async fn handle_intent(
    intent: Intent,
    state: &mut Model,
    profile: &mut Profile,
    service: &TunnelService,
    commands: &mut JoinSet<Option<String>>,
) {
    match intent {
        Intent::None => {}
        Intent::Exit => unreachable!("exit is handled by the event loop"),
        Intent::Primary => match state.tab {
            ActiveTab::Host => toggle_host(state, profile, service, commands).await,
            ActiveTab::Join => toggle_join(state, service, commands).await,
            ActiveTab::Relay => apply_relay(state, profile),
        },
        Intent::Stop => stop_tunnel(state, service, commands),
        Intent::SaveInputs => save_inputs(state, profile),
    }
}

async fn toggle_host(
    state: &mut Model,
    profile: &Profile,
    service: &TunnelService,
    commands: &mut JoinSet<Option<String>>,
) {
    match state.tunnel.state.phase {
        TunnelPhase::Idle => {}
        TunnelPhase::Active if state.tunnel.state.mode == Some(TunnelMode::Host) => {
            stop_tunnel(state, service, commands);
            return;
        }
        _ => {
            state.add_log("隧道运行中，请先停止当前隧道");
            return;
        }
    }

    let port = match state.host_port.value.parse::<u16>() {
        Ok(port) => port,
        Err(_) => {
            state.add_log("端口格式错误");
            return;
        }
    };
    let key_path = match sculk::persist::default_key_path() {
        Ok(path) => path,
        Err(e) => {
            state.add_log(&format!("密钥路径获取失败: {e}"));
            return;
        }
    };
    let secret_key = match sculk::persist::load_or_generate_key(&key_path) {
        Ok(key) => key,
        Err(e) => {
            state.add_log(&format!("密钥加载失败: {e}"));
            return;
        }
    };
    let custom_relay = (state.relay_idx == 1).then_some(state.relay_url.value.as_str());
    let relay_url = match profile.resolve_relay_url(custom_relay) {
        Ok(url) => url,
        Err(e) => {
            state.add_log(&format!("中继配置错误: {e}"));
            return;
        }
    };
    let password = non_empty(&state.host_password.value);
    let config = HostConfig::new()
        .event_delay(Duration::ZERO)
        .password(password);
    let options = HostOptions::new(port)
        .secret_key(Some(secret_key))
        .relay_url(relay_url)
        .config(config);

    state.quit_pressed_at = None;
    state.add_log(&format!("正在启动 host 隧道 (端口 {port})..."));
    if let Err(e) = service.start_host(options).await {
        state.add_log(&format!("host 启动失败: {e}"));
    }
}

async fn toggle_join(
    state: &mut Model,
    service: &TunnelService,
    commands: &mut JoinSet<Option<String>>,
) {
    match state.tunnel.state.phase {
        TunnelPhase::Idle => {}
        TunnelPhase::Active if state.tunnel.state.mode == Some(TunnelMode::Join) => {
            stop_tunnel(state, service, commands);
            return;
        }
        _ => {
            state.add_log("隧道运行中，请先停止当前隧道");
            return;
        }
    }

    let ticket = match state.join_ticket.value.trim().parse::<Ticket>() {
        Ok(ticket) => ticket,
        Err(e) => {
            state.add_log(&format!("票据解析失败: {e}"));
            return;
        }
    };
    let port = match state.join_port.value.parse::<u16>() {
        Ok(port) => port,
        Err(_) => {
            state.add_log("端口格式错误");
            return;
        }
    };
    let config = JoinConfig::new()
        .event_delay(Duration::ZERO)
        .password(non_empty(&state.join_password.value));

    state.quit_pressed_at = None;
    state.add_log("正在连接...");
    if let Err(e) = service
        .start_join(JoinOptions::new(ticket, port).config(config))
        .await
    {
        state.add_log(&format!("join 失败: {e}"));
    }
}

fn stop_tunnel(state: &mut Model, service: &TunnelService, commands: &mut JoinSet<Option<String>>) {
    state.quit_pressed_at = None;
    state.add_log("正在关闭隧道...");
    let service = service.clone();
    commands.spawn(async move {
        match time::timeout(Duration::from_secs(5), service.shutdown()).await {
            Ok(Ok(())) => None,
            Ok(Err(e)) => Some(format!("关闭隧道失败: {e}")),
            Err(_) => Some("关闭隧道超时 (5s)".to_string()),
        }
    });
}

fn apply_relay(state: &mut Model, profile: &mut Profile) {
    if state.tunnel.state.phase != TunnelPhase::Idle {
        state.add_log("隧道运行中，无法切换中继");
        return;
    }
    let selected = state.relay_state.selected().unwrap_or(state.relay_idx);
    if selected == state.relay_idx {
        state.add_log(&format!("中继保持不变: {}", RELAYS[state.relay_idx]));
        return;
    }
    match selected {
        0 => profile.relay.custom = false,
        1 => {
            let url = state.relay_url.value.trim();
            if url.is_empty() {
                state.add_log("请先输入自建中继 URL");
                return;
            }
            if let Err(e) = profile.resolve_relay_url(Some(url)) {
                state.add_log(&format!("保存失败: {e}"));
                return;
            }
            profile.relay.custom = true;
            profile.relay.url = Some(url.to_owned());
        }
        _ => return,
    }
    if let Err(e) = profile.save() {
        state.add_log(&format!("保存失败: {e}"));
        return;
    }
    state.relay_idx = selected;
    state.add_log(&format!("中继已切换到 {}", RELAYS[selected]));
}

fn save_inputs(state: &mut Model, profile: &mut Profile) {
    if let Ok(port) = state.host_port.value.parse::<u16>() {
        profile.host.port = port;
    }
    if let Ok(port) = state.join_port.value.parse::<u16>() {
        profile.join.port = port;
    }
    let relay_url = state.relay_url.value.trim();
    profile.relay.url = (!relay_url.is_empty()).then(|| relay_url.to_owned());
    if let Err(e) = profile.save() {
        state.add_log(&format!("配置保存失败: {e}"));
    }
}

fn remember_join_ticket(state: &mut Model, profile: &mut Profile, update: &TunnelUpdate) {
    let TunnelUpdate::Status(status) = update else {
        return;
    };
    if state.tunnel.state.phase == TunnelPhase::Starting
        && status.state.phase == TunnelPhase::Active
        && status.state.mode == Some(TunnelMode::Join)
    {
        profile.join.last_ticket = Some(state.join_ticket.value.clone());
        if let Err(e) = profile.save() {
            state.add_log(&format!("配置保存失败: {e}"));
        }
    }
}

fn load_profile() -> (Profile, Option<String>) {
    match Profile::load() {
        Ok(profile) => (profile, None),
        Err(e) => (Profile::default(), Some(format!("配置加载失败: {e}"))),
    }
}

fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

fn copy_host_ticket(state: &mut Model, update: &TunnelUpdate) {
    let TunnelUpdate::Status(status) = update else {
        return;
    };
    if state.tunnel.state.phase == TunnelPhase::Active
        || status.state.phase != TunnelPhase::Active
        || status.state.mode != Some(TunnelMode::Host)
    {
        return;
    }
    let Some(ticket) = status.state.ticket.as_ref() else {
        return;
    };
    if sculk::clipboard::clipboard_copy(&ticket.to_string()) {
        state.add_log("票据已复制到剪贴板");
    }
}
