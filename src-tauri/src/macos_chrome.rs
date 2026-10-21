use objc2::MainThreadOnly;
use objc2_app_kit::{
    NSBox, NSBoxType, NSColor, NSLayoutAttribute, NSLayoutConstraint, NSLayoutRelation,
    NSTitlePosition, NSWindow, NSWindowOrderingMode,
};
use objc2_foundation::{NSArray, NSSize};

pub fn install_separator(window: &NSWindow) {
    // Wry 已将 WKWebView 放进原生内容容器；把分隔线作为其上方的兄弟视图，
    // 让它不受网页滚动、刷新或 WebKit 内部子视图重排影响。
    let content = window
        .contentView()
        .expect("Wry 主窗口的原生内容视图必须已创建");
    let layout = window
        .contentLayoutGuide()
        .expect("主窗口必须有原生内容布局参考线");
    let separator = NSBox::new(window.mtm());
    separator.setBoxType(NSBoxType::Custom);
    separator.setTitlePosition(NSTitlePosition::NoTitle);
    separator.setContentViewMargins(NSSize::new(0.0, 0.0));
    separator.setBorderWidth(0.0);
    separator.setCornerRadius(0.0);
    separator.setTransparent(false);
    separator.setFillColor(&NSColor::colorWithSRGBRed_green_blue_alpha(
        39.0 / 255.0,
        39.0 / 255.0,
        42.0 / 255.0,
        1.0,
    ));
    separator.setWantsLayer(true);
    separator.setTranslatesAutoresizingMaskIntoConstraints(false);
    content.addSubview_positioned_relativeTo(&separator, NSWindowOrderingMode::Above, None);

    // FullSizeContentView 的内容视图可能延伸到标题栏中，须锚定未被标题栏遮挡的
    // contentLayoutGuide；普通窗口也沿用该系统布局，不硬编码标题栏高度。
    // Top 属性不依赖视图是否 flipped；固定 1 logical pt，在 Retina 上为 2px。
    // 内容容器保留 separator，激活后的约束由 AppKit 持有，随窗口宽度自动布局。
    let align_edge = |attribute| {
        // AppKit 明确允许 contentLayoutGuide 作为约束 item，但未公开其具体类；
        // separator 是 NSView，layout 是系统提供的合法 layout item，不作类型假设。
        unsafe {
            NSLayoutConstraint::constraintWithItem_attribute_relatedBy_toItem_attribute_multiplier_constant(
                &separator,
                attribute,
                NSLayoutRelation::Equal,
                Some(&layout),
                attribute,
                1.0,
                0.0,
            )
        }
    };
    let constraints = NSArray::from_retained_slice(&[
        align_edge(NSLayoutAttribute::Top),
        align_edge(NSLayoutAttribute::Leading),
        align_edge(NSLayoutAttribute::Trailing),
        separator.heightAnchor().constraintEqualToConstant(1.0),
    ]);
    NSLayoutConstraint::activateConstraints(&constraints);
}
