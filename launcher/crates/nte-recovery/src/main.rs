//! 原生 GTK 应急窗口, 不初始化 Tauri 或 WebKit.
use std::sync::mpsc;
use std::time::Duration;

use gtk::prelude::*;
use nte_core::{recovery, settings::UserPaths};

fn confirm(window: &gtk::Window, message: &str) -> bool {
    let dialog = gtk::MessageDialog::new(
        Some(window),
        gtk::DialogFlags::MODAL,
        gtk::MessageType::Warning,
        gtk::ButtonsType::OkCancel,
        message,
    );
    let accepted = dialog.run() == gtk::ResponseType::Ok;
    dialog.close();
    accepted
}

fn job(
    controls: &gtk::Box,
    output: &gtk::TextBuffer,
    run: impl FnOnce() -> anyhow::Result<String> + Send + 'static,
) {
    controls.set_sensitive(false);
    output.set_text("正在处理, 请稍候...");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(run().unwrap_or_else(|e| format!("操作失败: {e:#}")));
    });
    let controls = controls.clone();
    let output = output.clone();
    gtk::glib::timeout_add_local(Duration::from_millis(100), move || {
        match receiver.try_recv() {
            Ok(text) => {
                output.set_text(&text);
                controls.set_sensitive(true);
                gtk::glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => gtk::glib::ControlFlow::Continue,
            Err(_) => {
                output.set_text("应急任务异常结束, 请刷新诊断后重试");
                controls.set_sensitive(true);
                gtk::glib::ControlFlow::Break
            }
        }
    });
}

fn main() {
    if let Err(error) = gtk::init() {
        eprintln!("无法打开应急窗口: {error}. 无显示环境时请使用 nte-cli recovery diagnose");
        std::process::exit(1);
    }
    let _window = build_window();
    gtk::main();
}

fn build_window() -> gtk::Window {
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("Oddity 应急恢复");
    window.set_default_size(760, 560);
    window.set_position(gtk::WindowPosition::Center);
    let layout = gtk::Box::new(gtk::Orientation::Vertical, 12);
    layout.set_border_width(18);
    let title = gtk::Label::new(Some(
        "主界面无法使用时, 可在此检查环境、导出诊断并恢复界面设置.",
    ));
    title.set_xalign(0.0);
    title.set_line_wrap(true);
    layout.pack_start(&title, false, false, 0);
    let scroll = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    scroll.set_vexpand(true);
    let text = gtk::TextView::new();
    text.set_editable(false);
    text.set_cursor_visible(false);
    text.set_monospace(true);
    text.set_wrap_mode(gtk::WrapMode::WordChar);
    let output = text.buffer().expect("文本缓冲区");
    output.set_text(&recovery::diagnose(&UserPaths::discover()));
    scroll.add(&text);
    layout.pack_start(&scroll, true, true, 0);
    let controls = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let first = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let second = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    for row in [&first, &second] {
        controls.pack_start(row, false, false, 0);
    }
    for (label, action, row) in [
        ("刷新诊断", "diagnose", &first),
        ("导出诊断包", "export", &first),
        ("兼容启动", "start", &first),
        ("结束启动器进程", "stop", &second),
        ("恢复界面设置", "reset", &second),
        ("重建损坏配置", "rebuild", &second),
    ] {
        let button = gtk::Button::with_label(label);
        row.pack_start(&button, true, true, 0);
        let window = window.clone();
        let controls = controls.clone();
        let output = output.clone();
        button.connect_clicked(move |_| {
            let paths = UserPaths::discover();
            match action {
                "diagnose" => output.set_text(&recovery::diagnose(&paths)),
                "export" => {
                    let chooser = gtk::FileChooserDialog::new(Some("选择诊断包保存目录"), Some(&window), gtk::FileChooserAction::SelectFolder);
                    chooser.add_buttons(&[("取消", gtk::ResponseType::Cancel), ("保存", gtk::ResponseType::Accept)]);
                    let directory = if chooser.run() == gtk::ResponseType::Accept { chooser.filename() } else { None };
                    chooser.close();
                    if let Some(directory) = directory {
                        job(&controls, &output, move || Ok(format!("诊断包已保存: {}\n日志可能包含路径和账号信息, 分享前请检查.", recovery::export_diagnostics(&paths, &directory)?.display())));
                    }
                }
                "start" => job(&controls, &output, move || {
                    let mut child = recovery::start_compatible(&paths)?;
                    let pid = child.id();
                    std::thread::spawn(move || { let _ = child.wait(); });
                    Ok(format!("已发起兼容启动, PID={pid}. 请检查主界面是否可用.\n本次使用静态背景和保守渲染设置. 如仍无法显示, 请导出诊断包."))
                }),
                "stop" => {
                    if confirm(&window, "将终止当前登记的 Oddity 启动器进程, 可能中断下载或其他正在进行的任务. 确定继续?") {
                        job(&controls, &output, move || { recovery::stop_launcher(&paths)?; Ok("启动器已退出. 可以恢复设置或重新启动.".into()) });
                    }
                }
                "reset" | "rebuild" => {
                    let rebuild = action == "rebuild";
                    let message = if rebuild {
                        "将备份原配置并重建无法解析的设置. 损坏配置中的安装目录、Proton 等选项需重新选择; 游戏文件和账号凭据不会被删除. 确定继续?"
                    } else { "将备份原配置, 恢复静态背景、动画和窗口行为等界面设置. 安装目录、Proton 和账号凭据将保留. 确定继续?" };
                    if confirm(&window, message) {
                        job(&controls, &output, move || {
                            let backup = recovery::reset_interface(&paths, rebuild)?;
                            Ok(format!("界面设置已恢复.\n备份: {}", backup.map_or_else(|| "原配置不存在".into(), |p| p.display().to_string())))
                        });
                    }
                }
                _ => {}
            }
        });
    }
    layout.pack_start(&controls, false, false, 0);
    let note = gtk::Label::new(Some(
        "兼容启动仅影响本次运行. 修改配置前请先退出主程序; 恢复操作会保留原配置备份.",
    ));
    note.set_xalign(0.0);
    note.set_line_wrap(true);
    layout.pack_start(&note, false, false, 0);
    window.add(&layout);
    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        gtk::glib::Propagation::Proceed
    });
    window.show_all();
    window
}

#[cfg(test)]
mod tests;
