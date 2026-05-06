// Borderless NSPanel поверх всего, SwiftUI-список истории.
// Стрелки навигируют, Enter / клик выбирают, Esc закрывает (с возвратом фокуса).
// Клик мышью вне панели — закрыть без возврата фокуса (юзер уже куда-то кликнул).

import AppKit
import Carbon.HIToolbox
import SwiftUI

final class KeyablePanel: NSPanel {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }
}

final class PopupWindow {
    private var panel: KeyablePanel?
    private var resignObserver: NSObjectProtocol?
    private let store: HistoryStore
    private let onPick: (ClipItem) -> Void
    private let onCancel: () -> Void
    private let onClickOutside: () -> Void

    init(
        store: HistoryStore,
        onPick: @escaping (ClipItem) -> Void,
        onCancel: @escaping () -> Void,
        onClickOutside: @escaping () -> Void
    ) {
        self.store = store
        self.onPick = onPick
        self.onCancel = onCancel
        self.onClickOutside = onClickOutside
    }

    func toggle() {
        if panel != nil {
            Log.write("[popup]", "toggle → hide")
            hide()
            return
        }
        Log.write("[popup]", "toggle → show")
        show()
    }

    private func show() {
        let view = PopupView(
            store: store,
            onPick: { [weak self] item in
                self?.hide()
                self?.onPick(item)
            },
            onCancel: { [weak self] in
                self?.hide()
                self?.onCancel()
            }
        )
        let host = NSHostingView(rootView: view)

        let p = KeyablePanel(
            contentRect: NSRect(x: 0, y: 0, width: 280, height: 280),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        p.level = .floating
        p.isFloatingPanel = true
        p.hidesOnDeactivate = false
        p.backgroundColor = .clear
        p.isOpaque = false
        p.hasShadow = true
        p.contentView = host
        p.center()

        // Намеренно НЕ зовём NSApp.activate() — иначе крадём фокус у активного окна,
        // и потом синтетический Cmd+V прилетает не туда. nonactivatingPanel + canBecomeKey
        // даёт нам клавиатурный ввод без активации приложения.
        p.makeKeyAndOrderFront(nil)
        panel = p

        // Терминалы (Ghostty / iTerm2 / kitty) при зажатом Option показывают
        // crosshair-курсор для прямоугольного выделения. Когда popup становится
        // key-window, исходное окно не получает Option-up и зависает в этом режиме.
        // Синтезируем Option-up чтобы предыдущему окну явно сообщить «отпустили».
        releaseOptionModifier()

        // Дисмисс при потере key-status (клик в другое окно / Cmd+Tab).
        // Чище чем NSEvent global monitor: не консьюмим события и не путаем системные модификаторы.
        resignObserver = NotificationCenter.default.addObserver(
            forName: NSWindow.didResignKeyNotification,
            object: p,
            queue: .main
        ) { [weak self] _ in
            Log.write("[popup]", "didResignKey → dismiss")
            self?.hide()
            self?.onClickOutside()
        }
        Log.write("[popup]", "shown, isKey=\(p.isKeyWindow)")
    }

    func hide() {
        Log.write("[popup]", "hide()")
        if let obs = resignObserver {
            NotificationCenter.default.removeObserver(obs)
            resignObserver = nil
        }
        panel?.orderOut(nil)
        panel = nil
    }

    private func releaseOptionModifier() {
        guard let src = CGEventSource(stateID: .combinedSessionState) else { return }
        for kc in [CGKeyCode(kVK_Option), CGKeyCode(kVK_RightOption)] {
            if let up = CGEvent(keyboardEventSource: src, virtualKey: kc, keyDown: false) {
                up.flags = []
                up.post(tap: .cghidEventTap)
            }
        }
    }
}

private struct PopupView: View {
    @ObservedObject var store: HistoryStore
    let onPick: (ClipItem) -> Void
    let onCancel: () -> Void
    @State private var selection: Int = 0
    @FocusState private var focused: Bool

    var body: some View {
        VStack(spacing: 0) {
            if store.items.isEmpty {
                Text("История пуста")
                    .foregroundColor(.secondary)
                    .font(.system(size: 11))
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollViewReader { proxy in
                    ScrollView {
                        VStack(spacing: 2) {
                            ForEach(Array(store.items.enumerated()), id: \.element.id) { idx, item in
                                ItemRow(item: item, selected: idx == selection)
                                    .id(idx)
                                    .contentShape(Rectangle())
                                    .onTapGesture { onPick(item) }
                            }
                        }
                        .padding(6)
                    }
                    .onChange(of: selection) { _, new in
                        withAnimation(.linear(duration: 0.05)) {
                            proxy.scrollTo(new, anchor: .center)
                        }
                    }
                }
            }
        }
        .frame(width: 280, height: 280)
        .background(.regularMaterial)
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .focusable()
        .focused($focused)
        .onAppear {
            selection = 0
            DispatchQueue.main.async { focused = true }
        }
        .onKeyPress(.upArrow) {
            selection = max(0, selection - 1)
            return .handled
        }
        .onKeyPress(.downArrow) {
            selection = min(max(store.items.count - 1, 0), selection + 1)
            return .handled
        }
        .onKeyPress(.return) {
            if store.items.indices.contains(selection) {
                onPick(store.items[selection])
            }
            return .handled
        }
        .onKeyPress(.escape) {
            onCancel()
            return .handled
        }
    }
}

private struct ItemRow: View {
    let item: ClipItem
    let selected: Bool

    var body: some View {
        HStack(alignment: .center, spacing: 8) {
            switch item.kind {
            case .text(let s):
                Text(snippet(s))
                    .font(.system(size: 12))
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .foregroundColor(.primary)
            case .image(let img):
                Image(nsImage: img)
                    .resizable()
                    .aspectRatio(contentMode: .fit)
                    .frame(maxWidth: 80, maxHeight: 40)
                    .clipShape(RoundedRectangle(cornerRadius: 3))
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 5)
                .fill(selected ? Color.accentColor.opacity(0.35) : Color.clear)
        )
    }

    private func snippet(_ s: String) -> String {
        let oneLine = s.replacingOccurrences(of: "\n", with: " ")
            .replacingOccurrences(of: "\r", with: " ")
            .trimmingCharacters(in: .whitespaces)
        return String(oneLine.prefix(20))
    }
}
