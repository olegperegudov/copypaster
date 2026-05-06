// Стдерр-логгер. Запускай из терминала чтобы видеть события:
//   ~/copypaster/CopyPaster.app/Contents/MacOS/CopyPaster

import Foundation

enum Log {
    private static let formatter: DateFormatter = {
        let f = DateFormatter()
        f.dateFormat = "HH:mm:ss.SSS"
        return f
    }()

    static func write(_ tag: String, _ msg: String) {
        let line = "[\(formatter.string(from: Date()))] \(tag) \(msg)\n"
        FileHandle.standardError.write(Data(line.utf8))
    }
}
