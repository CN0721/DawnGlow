//! Android 平台胶水：运行时权限、位置读取与窗口安全区（纯 JNI，无 Java 代码）。

use android_activity::AndroidApp;
use jni::objects::{JObject, JValue};
use jni::JavaVM;

fn sdk_int(env: &mut jni::JNIEnv) -> i32 {
    env.get_static_field("android/os/Build$VERSION", "SDK_INT", "I")
        .ok()
        .and_then(|v| v.i().ok())
        .unwrap_or(30)
}

/// 让窗口延伸到系统栏 / 挖孔之下（边到边），随后由 UI 按 insets 自行留白。
/// 这样无论设备是否强制边到边，留白计算都一致、可控。
pub fn enable_edge_to_edge(app: &AndroidApp) {
    match try_edge_to_edge(app) {
        Ok(()) => log::info!("已启用边到边显示"),
        Err(e) => log::warn!("启用边到边失败（将使用默认留白）: {e}"),
    }
}

fn try_edge_to_edge(app: &AndroidApp) -> Result<(), String> {
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) }
        .map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
    let sdk = sdk_int(&mut env);

    let window = env
        .call_method(&activity, "getWindow", "()Landroid/view/Window;", &[])
        .and_then(|v| v.l())
        .map_err(|e| e.to_string())?;
    if window.is_null() {
        return Err("无 Window".into());
    }

    if sdk >= 30 {
        env.call_method(
            &window,
            "setDecorFitsSystemWindows",
            "(Z)V",
            &[JValue::Bool(0)],
        )
        .map_err(|e| e.to_string())?;
    } else {
        let decor = env
            .call_method(&window, "getDecorView", "()Landroid/view/View;", &[])
            .and_then(|v| v.l())
            .map_err(|e| e.to_string())?;
        // LAYOUT_STABLE | LAYOUT_HIDE_NAVIGATION | LAYOUT_FULLSCREEN | LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
        let flags = 256 | 512 | 1024 | 4096;
        env.call_method(&decor, "setSystemUiVisibility", "(I)V", &[JValue::Int(flags)])
            .map_err(|e| e.to_string())?;
    }

    // 系统栏透明，让渐变铺满；并强制浅色图标以适配深色界面。
    let _ = env.call_method(&window, "setStatusBarColor", "(I)V", &[JValue::Int(0)]);
    let _ = env.call_method(&window, "setNavigationBarColor", "(I)V", &[JValue::Int(0)]);
    if sdk >= 30 {
        if let Ok(ctrl) = env
            .call_method(
                &window,
                "getInsetsController",
                "()Landroid/view/WindowInsetsController;",
                &[],
            )
            .and_then(|v| v.l())
        {
            if !ctrl.is_null() {
                // APPEARANCE_LIGHT_STATUS_BARS=8, APPEARANCE_LIGHT_NAVIGATION_BARS=16（置 0 即清除，用浅色图标）
                let _ = env.call_method(
                    &ctrl,
                    "setSystemBarsAppearance",
                    "(II)V",
                    &[JValue::Int(0), JValue::Int(8 | 16)],
                );
            }
        }
    }
    let _ = env.exception_clear();
    Ok(())
}

/// 从资源读状态栏高度（像素），作为 WindowInsets 不可用时的兜底。
fn status_bar_height_px(env: &mut jni::JNIEnv, activity: &JObject) -> f32 {
    let res = match env
        .call_method(activity, "getResources", "()Landroid/content/res/Resources;", &[])
        .and_then(|v| v.l())
    {
        Ok(r) => r,
        Err(_) => {
            let _ = env.exception_clear();
            return 0.0;
        }
    };
    if res.is_null() {
        return 0.0;
    }
    let name: JObject = match env.new_string("status_bar_height") { Ok(s) => s.into(), Err(_) => return 0.0 };
    let def_type: JObject = match env.new_string("dimen") { Ok(s) => s.into(), Err(_) => return 0.0 };
    let pkg: JObject = match env.new_string("android") { Ok(s) => s.into(), Err(_) => return 0.0 };
    let id = env
        .call_method(
            &res,
            "getIdentifier",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)I",
            &[JValue::Object(&name), JValue::Object(&def_type), JValue::Object(&pkg)],
        )
        .and_then(|v| v.i())
        .unwrap_or(0);
    if id <= 0 {
        let _ = env.exception_clear();
        return 0.0;
    }
    let px = env
        .call_method(&res, "getDimensionPixelSize", "(I)I", &[JValue::Int(id)])
        .and_then(|v| v.i())
        .unwrap_or(0);
    px.max(0) as f32
}

/// 读取窗口安全区，返回 (left, top, right, bottom)，单位为物理像素。
pub fn window_insets(app: &AndroidApp) -> Result<(f32, f32, f32, f32), String> {
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) }
        .map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
    let sdk = sdk_int(&mut env);

    let window = env
        .call_method(&activity, "getWindow", "()Landroid/view/Window;", &[])
        .and_then(|v| v.l())
        .map_err(|e| e.to_string())?;
    let decor = env
        .call_method(&window, "getDecorView", "()Landroid/view/View;", &[])
        .and_then(|v| v.l())
        .map_err(|e| e.to_string())?;
    let insets = env
        .call_method(&decor, "getRootWindowInsets", "()Landroid/view/WindowInsets;", &[])
        .and_then(|v| v.l())
        .map_err(|e| e.to_string())?;
    if insets.is_null() {
        return Err("WindowInsets 尚未就绪".into());
    }

    let (l, t, r, b) = if sdk >= 30 {
        // WindowInsets.Type.systemBars() = 7, displayCutout() = 128
        let mask = 7 | 128;
        let obj = env
            .call_method(
                &insets,
                "getInsets",
                "(I)Landroid/graphics/Insets;",
                &[JValue::Int(mask)],
            )
            .and_then(|v| v.l())
            .map_err(|e| e.to_string())?;
        if obj.is_null() {
            return Err("Insets 为空".into());
        }
        let l = env.get_field(&obj, "left", "I").and_then(|v| v.i()).unwrap_or(0) as f32;
        let t = env.get_field(&obj, "top", "I").and_then(|v| v.i()).unwrap_or(0) as f32;
        let r = env.get_field(&obj, "right", "I").and_then(|v| v.i()).unwrap_or(0) as f32;
        let b = env.get_field(&obj, "bottom", "I").and_then(|v| v.i()).unwrap_or(0) as f32;
        (l, t, r, b)
    } else {
        let mut l = env.call_method(&insets, "getSystemWindowInsetLeft", "()I", &[]).and_then(|v| v.i()).unwrap_or(0) as f32;
        let mut t = env.call_method(&insets, "getSystemWindowInsetTop", "()I", &[]).and_then(|v| v.i()).unwrap_or(0) as f32;
        let mut r = env.call_method(&insets, "getSystemWindowInsetRight", "()I", &[]).and_then(|v| v.i()).unwrap_or(0) as f32;
        let mut b = env.call_method(&insets, "getSystemWindowInsetBottom", "()I", &[]).and_then(|v| v.i()).unwrap_or(0) as f32;
        // DisplayCutout 仅 API 28+ 存在；失败会留下挂起异常，需清理。
        if sdk >= 28 {
            let dc = env
                .call_method(&insets, "getDisplayCutout", "()Landroid/view/DisplayCutout;", &[])
                .and_then(|v| v.l());
            match dc {
                Ok(dc) if !dc.is_null() => {
                    l = l.max(env.call_method(&dc, "getSafeInsetLeft", "()I", &[]).and_then(|v| v.i()).unwrap_or(0) as f32);
                    t = t.max(env.call_method(&dc, "getSafeInsetTop", "()I", &[]).and_then(|v| v.i()).unwrap_or(0) as f32);
                    r = r.max(env.call_method(&dc, "getSafeInsetRight", "()I", &[]).and_then(|v| v.i()).unwrap_or(0) as f32);
                    b = b.max(env.call_method(&dc, "getSafeInsetBottom", "()I", &[]).and_then(|v| v.i()).unwrap_or(0) as f32);
                }
                _ => {
                    let _ = env.exception_clear();
                }
            }
        }
        (l, t, r, b)
    };
    // 全零（查询无结果）时用状态栏高度兜底，避免顶部被系统栏遮挡。
    let mut t = t;
    if l + t + r + b == 0.0 {
        t = status_bar_height_px(&mut env, &activity);
    }
    Ok((l, t, r, b))
}

/// 请求定位权限（Coarse + Fine）。尽力而为，失败只记录日志。
pub fn request_location_permission(app: &AndroidApp) {
    match try_request(app) {
        Ok(()) => log::info!("已发起定位权限请求"),
        Err(e) => log::warn!("请求定位权限失败（可在系统设置中手动授予）: {e}"),
    }
}

fn try_request(app: &AndroidApp) -> Result<(), String> {
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) }
        .map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };

    let perms = [
        "android.permission.ACCESS_FINE_LOCATION",
        "android.permission.ACCESS_COARSE_LOCATION",
    ];
    let null = JObject::null();
    let array = env
        .new_object_array(perms.len() as i32, "java/lang/String", &null)
        .map_err(|e| e.to_string())?;
    for (i, p) in perms.iter().enumerate() {
        let s: JObject = env.new_string(p).map_err(|e| e.to_string())?.into();
        env.set_object_array_element(&array, i as i32, &s)
            .map_err(|e| e.to_string())?;
    }

    env.call_method(
        &activity,
        "requestPermissions",
        "([Ljava/lang/String;I)V",
        &[JValue::Object(&array), JValue::Int(1002)],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 读取系统缓存的最近一次定位（gps / network / passive / fused 依次尝试）。
///
/// NativeActivity 无法注册 Java LocationListener，故只能取「最近已知位置」；
/// 无缓存时由上层提示改用地图选点。
pub fn last_location(app: &AndroidApp) -> Result<(f64, f64), String> {
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) }
        .map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };

    let service_str: JObject = env.new_string("location").map_err(|e| e.to_string())?.into();
    let service = env
        .call_method(
            &activity,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&service_str)],
        )
        .and_then(|v| v.l())
        .map_err(|e| e.to_string())?;
    if service.is_null() {
        return Err("无法获取 LocationManager".into());
    }

    for provider in ["gps", "network", "passive", "fused"] {
        let p: JObject = env.new_string(provider).map_err(|e| e.to_string())?.into();
        let loc = env
            .call_method(
                &service,
                "getLastKnownLocation",
                "(Ljava/lang/String;)Landroid/location/Location;",
                &[JValue::Object(&p)],
            )
            .and_then(|v| v.l());
        let Ok(loc) = loc else { continue };
        if loc.is_null() {
            continue;
        }
        let lat = env
            .call_method(&loc, "getLatitude", "()D", &[])
            .and_then(|v| v.d())
            .map_err(|e| e.to_string())?;
        let lon = env
            .call_method(&loc, "getLongitude", "()D", &[])
            .and_then(|v| v.d())
            .map_err(|e| e.to_string())?;
        log::info!("定位成功（{provider}）: {lat:.4},{lon:.4}");
        return Ok((lat, lon));
    }
    Err("系统暂无缓存定位，请到开阔处或改用手动选点".into())
}

/// 结束当前 Activity（首页按系统返回时的行为）。必须主线程调用。
pub fn finish_activity(app: &AndroidApp) {
    match try_finish(app) {
        Ok(()) => log::info!("finish activity"),
        Err(e) => log::warn!("finish 失败: {e}"),
    }
}

fn try_finish(app: &AndroidApp) -> Result<(), String> {
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) }
        .map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
    env.call_method(&activity, "finish", "()V", &[])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 桌面小组件入口：`WidgetBridge.forecast(lat, lon, filesDir)`。
///
/// 复用 Rust 的数据层与评分，返回多行摘要文本（失败返回错误/缓存）。
#[no_mangle]
pub extern "system" fn Java_top_izuna_dawnglow_WidgetBridge_forecast(
    mut env: jni::JNIEnv,
    _class: jni::objects::JClass,
    lat: jni::sys::jdouble,
    lon: jni::sys::jdouble,
    files_dir: jni::objects::JString,
) -> jni::sys::jstring {
    let dir = env
        .get_string(&files_dir)
        .map(String::from)
        .ok()
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let text = crate::widget::summary(lat, lon, &dir);
    match env.new_string(text) {
        Ok(s) => s.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}
