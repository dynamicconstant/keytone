import AVFoundation
import Foundation

/// Key press (`down`) or release (`up`).
enum KeyDir {
    case down, up
}

/// Mirrors the desktop `pack.json` format (see docs/PACK_FORMAT.md).
struct Slot: Decodable {
    var down: [String]?
    var up: [String]?

    func refs(_ dir: KeyDir) -> [String]? {
        dir == .down ? down : up
    }
}

struct RandomCfg: Decodable {
    var pitchCents: Double = 30
    var gainDb: Double = 1.5

    enum CodingKeys: String, CodingKey {
        case pitchCents = "pitch_cents"
        case gainDb = "gain_db"
    }

    init() {}

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        pitchCents = try c.decodeIfPresent(Double.self, forKey: .pitchCents) ?? 30
        gainDb = try c.decodeIfPresent(Double.self, forKey: .gainDb) ?? 1.5
    }
}

struct Sprite: Decodable {
    var file: String
    var segments: [String: [Double]]
}

struct PackFile: Decodable {
    var id: String
    var name: String
    var author: String
    var description: String
    var volume: Double
    var trimSilence: Bool
    var random: RandomCfg
    var sprite: Sprite?
    var defaults: Slot
    var groups: [String: Slot]
    var keys: [String: Slot]

    enum CodingKeys: String, CodingKey {
        case id, name, author, description, volume, random, sprite, defaults, groups, keys
        case trimSilence = "trim_silence"
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        name = try c.decode(String.self, forKey: .name)
        author = try c.decodeIfPresent(String.self, forKey: .author) ?? ""
        description = try c.decodeIfPresent(String.self, forKey: .description) ?? ""
        volume = try c.decodeIfPresent(Double.self, forKey: .volume) ?? 1
        trimSilence = try c.decodeIfPresent(Bool.self, forKey: .trimSilence) ?? true
        random = try c.decodeIfPresent(RandomCfg.self, forKey: .random) ?? RandomCfg()
        sprite = try c.decodeIfPresent(Sprite.self, forKey: .sprite)
        defaults = try c.decodeIfPresent(Slot.self, forKey: .defaults) ?? Slot()
        groups = try c.decodeIfPresent([String: Slot].self, forKey: .groups) ?? [:]
        keys = try c.decodeIfPresent([String: Slot].self, forKey: .keys) ?? [:]
    }

    /// Resolution order: key -> group -> defaults (same as desktop).
    func refs(for code: String, _ dir: KeyDir) -> [String] {
        if let r = keys[code]?.refs(dir) { return r }
        if let r = groups[KeyCodes.group(of: code)]?.refs(dir) { return r }
        return defaults.refs(dir) ?? []
    }

    var allRefs: Set<String> {
        var s = Set<String>()
        for slot in [defaults] + Array(groups.values) + Array(keys.values) {
            slot.down?.forEach { s.insert($0) }
            slot.up?.forEach { s.insert($0) }
        }
        return s
    }
}

struct PackInfo: Identifiable, Hashable {
    let id: String
    let name: String
    let description: String
    let url: URL
}

enum PackLibrary {
    static var root: URL? {
        Bundle.main.url(forResource: "packs", withExtension: nil)
    }

    static func all() -> [PackInfo] {
        guard let root, let dirs = try? FileManager.default.contentsOfDirectory(
            at: root, includingPropertiesForKeys: nil) else { return [] }
        return dirs.compactMap { dir -> PackInfo? in
            guard let data = try? Data(contentsOf: dir.appendingPathComponent("pack.json")),
                  let meta = try? JSONDecoder().decode(PackFile.self, from: data) else { return nil }
            return PackInfo(id: dir.lastPathComponent, name: meta.name, description: meta.description, url: dir)
        }
        .sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
    }
}

/// A pack with every clip decoded into memory, ready to play instantly.
final class LoadedPack {
    let id: String
    let meta: PackFile
    private var buffers: [String: AVAudioPCMBuffer] = [:]

    init?(info: PackInfo, format: AVAudioFormat) {
        guard let data = try? Data(contentsOf: info.url.appendingPathComponent("pack.json")),
              let meta = try? JSONDecoder().decode(PackFile.self, from: data) else { return nil }
        id = info.id
        self.meta = meta

        var sprite: AVAudioPCMBuffer?
        if let sp = meta.sprite {
            sprite = AudioClip.load(url: info.url.appendingPathComponent(sp.file), format: format)
        }
        // Packs louder than 1.0 are boosted in the samples (player volume caps at 1).
        let boost = Float(max(1.0, meta.volume))
        for ref in meta.allRefs {
            var buf: AVAudioPCMBuffer?
            if ref.hasPrefix("#") {
                let seg = String(ref.dropFirst())
                if let sprite, let s = meta.sprite?.segments[seg], s.count == 2 {
                    buf = AudioClip.slice(sprite, startMs: s[0], durationMs: s[1])
                }
            } else if !ref.contains("/"), !ref.contains("..") {
                buf = AudioClip.load(url: info.url.appendingPathComponent(ref), format: format)
            }
            guard var b = buf else { continue }
            if meta.trimSilence, let t = AudioClip.trimLeadingSilence(b) { b = t }
            if boost > 1 { AudioClip.scale(b, by: boost) }
            if b.frameLength > 0 { buffers[ref] = b }
        }
    }

    func candidates(for code: String, _ dir: KeyDir) -> [AVAudioPCMBuffer] {
        meta.refs(for: code, dir).compactMap { buffers[$0] }
    }

    var playerGain: Float { Float(min(1.0, meta.volume)) }
}

enum AudioClip {
    /// Engine format: 44.1 kHz stereo float.
    static let format = AVAudioFormat(standardFormatWithSampleRate: 44_100, channels: 2)!

    static func load(url: URL, format: AVAudioFormat) -> AVAudioPCMBuffer? {
        guard let file = try? AVAudioFile(forReading: url) else { return nil }
        let frames = AVAudioFrameCount(file.length)
        guard frames > 0, frames < 44_100 * 300,
              let input = AVAudioPCMBuffer(pcmFormat: file.processingFormat, frameCapacity: frames) else { return nil }
        do { try file.read(into: input) } catch { return nil }
        return convert(input, to: format)
    }

    static func convert(_ input: AVAudioPCMBuffer, to format: AVAudioFormat) -> AVAudioPCMBuffer? {
        if input.format == format { return input }
        guard let conv = AVAudioConverter(from: input.format, to: format) else { return nil }
        if input.format.channelCount == 1 && format.channelCount == 2 {
            conv.channelMap = [0, 0]
        }
        let ratio = format.sampleRate / input.format.sampleRate
        let capacity = AVAudioFrameCount(Double(input.frameLength) * ratio) + 2048
        guard let out = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: capacity) else { return nil }
        var fed = false
        var error: NSError?
        let status = conv.convert(to: out, error: &error) { _, outStatus in
            if fed {
                outStatus.pointee = .endOfStream
                return nil
            }
            fed = true
            outStatus.pointee = .haveData
            return input
        }
        return status == .error ? nil : out
    }

    static func slice(_ src: AVAudioPCMBuffer, startMs: Double, durationMs: Double) -> AVAudioPCMBuffer? {
        let sr = src.format.sampleRate
        let start = min(Int(src.frameLength), Int(startMs / 1000 * sr))
        let count = min(Int(src.frameLength) - start, Int(durationMs / 1000 * sr))
        return copy(src, from: start, count: count)
    }

    static func copy(_ src: AVAudioPCMBuffer, from start: Int, count: Int) -> AVAudioPCMBuffer? {
        guard count > 0, let data = src.floatChannelData,
              let out = AVAudioPCMBuffer(pcmFormat: src.format, frameCapacity: AVAudioFrameCount(count)),
              let dst = out.floatChannelData else { return nil }
        for ch in 0..<Int(src.format.channelCount) {
            dst[ch].update(from: data[ch].advanced(by: start), count: count)
        }
        out.frameLength = AVAudioFrameCount(count)
        return out
    }

    /// Drop leading silence (keeps ~1 ms) so sounds feel instant.
    static func trimLeadingSilence(_ b: AVAudioPCMBuffer, threshold: Float = 0.003) -> AVAudioPCMBuffer? {
        guard let data = b.floatChannelData else { return nil }
        let n = Int(b.frameLength)
        let chans = Int(b.format.channelCount)
        var first = n
        outer: for i in 0..<n {
            for ch in 0..<chans where abs(data[ch][i]) > threshold {
                first = i
                break outer
            }
        }
        let cut = max(0, first - Int(b.format.sampleRate / 1000))
        if cut == 0 { return b }
        if first == n { return nil }
        return copy(b, from: cut, count: n - cut)
    }

    static func scale(_ b: AVAudioPCMBuffer, by g: Float) {
        guard let data = b.floatChannelData else { return }
        for ch in 0..<Int(b.format.channelCount) {
            for i in 0..<Int(b.frameLength) {
                data[ch][i] = max(-1, min(1, data[ch][i] * g))
            }
        }
    }
}

/// W3C `KeyboardEvent.code` groups, identical to the desktop app.
enum KeyCodes {
    static func group(of code: String) -> String {
        if code.hasPrefix("Key") { return "letters" }
        if code.hasPrefix("Digit") { return "digits" }
        if code == "NumpadEnter" { return "enter" }
        if code.hasPrefix("Numpad") || code == "NumLock" { return "numpad" }
        switch code {
        case "Minus", "Equal", "BracketLeft", "BracketRight", "Backslash", "Semicolon", "Quote",
             "Backquote", "Comma", "Period", "Slash", "IntlBackslash":
            return "punctuation"
        case "ShiftLeft", "ShiftRight", "ControlLeft", "ControlRight", "AltLeft", "AltRight",
             "MetaLeft", "MetaRight", "CapsLock", "Fn":
            return "modifiers"
        case "Space": return "space"
        case "Enter": return "enter"
        case "Backspace", "Delete": return "backspace"
        case "Tab": return "tab"
        case "Escape": return "escape"
        case "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight": return "arrows"
        case "Home", "End", "PageUp", "PageDown", "Insert": return "navigation"
        default: break
        }
        if code.count <= 3, code.hasPrefix("F"), Int(code.dropFirst()) != nil { return "function" }
        return "other"
    }

    /// The physical key a typed character lives on (US layout), so packs
    /// that give each key its own sound (Piano, Marimba, Drum Kit) work.
    static func code(for ch: String) -> String {
        let lower = ch.lowercased()
        if lower.count == 1, let c = lower.first {
            if c >= "a" && c <= "z" { return "Key" + String(c).uppercased() }
            if c >= "0" && c <= "9" { return "Digit" + String(c) }
        }
        let map: [String: String] = [
            "!": "Digit1", "@": "Digit2", "#": "Digit3", "$": "Digit4", "%": "Digit5", "^": "Digit6",
            "&": "Digit7", "*": "Digit8", "(": "Digit9", ")": "Digit0", "-": "Minus", "_": "Minus",
            "=": "Equal", "+": "Equal", "[": "BracketLeft", "{": "BracketLeft", "]": "BracketRight",
            "}": "BracketRight", "\\": "Backslash", "|": "Backslash", ";": "Semicolon", ":": "Semicolon",
            "'": "Quote", "\"": "Quote", "`": "Backquote", "~": "Backquote", ",": "Comma", "<": "Comma",
            ".": "Period", ">": "Period", "/": "Slash", "?": "Slash",
        ]
        return map[ch] ?? "IntlBackslash"
    }
}
