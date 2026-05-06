// Carbon RegisterEventHotKey wrapper для глобального Option+V.
// Carbon не deprecated для hotkeys (NSEvent global monitor не блокирует событие, что критично для Option+V).
// Без Accessibility разрешений — Carbon API работает на уровне WindowServer.

import Carbon.HIToolbox

final class HotKeyMonitor {
    private var hotKeyRef: EventHotKeyRef?
    private var handlerRef: EventHandlerRef?
    private let onPress: () -> Void

    init(onPress: @escaping () -> Void) {
        self.onPress = onPress
    }

    func install() {
        var eventType = EventTypeSpec(
            eventClass: OSType(kEventClassKeyboard),
            eventKind: UInt32(kEventHotKeyPressed)
        )

        let selfPtr = Unmanaged.passUnretained(self).toOpaque()

        InstallEventHandler(
            GetApplicationEventTarget(),
            { _, _, userData in
                guard let userData else { return noErr }
                let me = Unmanaged<HotKeyMonitor>.fromOpaque(userData).takeUnretainedValue()
                DispatchQueue.main.async { me.onPress() }
                return noErr
            },
            1,
            &eventType,
            selfPtr,
            &handlerRef
        )

        // Signature 'CPST' (0x43505354), id=1.
        let id = EventHotKeyID(signature: 0x43505354, id: 1)
        // kVK_ANSI_V = 9, optionKey = 1 << 11.
        let status = RegisterEventHotKey(
            UInt32(kVK_ANSI_V),
            UInt32(optionKey),
            id,
            GetApplicationEventTarget(),
            0,
            &hotKeyRef
        )
        Log.write("[hotkey]", "RegisterEventHotKey status=\(status) (0=ok)")
    }
}
