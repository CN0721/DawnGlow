package top.izuna.dawnglow;

/**
 * 桥接 Rust 数据层，供桌面小组件复用同一条火烧云预报链路。
 *
 * <p>实现位于 Rust 侧（android_glue.rs 的 JNI 入口），避免在 Java 里重复实现
 * Open-Meteo 请求与评分。
 */
public final class WidgetBridge {
    static {
        System.loadLibrary("dawnglow");
    }

    private WidgetBridge() {}

    /**
     * 返回多行预报摘要（日期 / 朝霞 / 晚霞 / 气溶胶）。
     *
     * @param lat       纬度
     * @param lon       经度
     * @param filesDir  应用私有 files 目录，用于离线缓存
     */
    public static native String forecast(double lat, double lon, String filesDir);
}
