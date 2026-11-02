import AVFoundation
import Foundation

/// Low-latency clip player: a small pool of player nodes, each through a
/// varispeed unit (for per-press pitch variation), mixed together.
final class SoundEngine {
    private let engine = AVAudioEngine()
    private var voices: [(player: AVAudioPlayerNode, speed: AVAudioUnitVarispeed)] = []
    private var nextVoice = 0
    private var configured = false

    var volume: Float {
        get { engine.mainMixerNode.outputVolume }
        set { engine.mainMixerNode.outputVolume = max(0, min(1, newValue)) }
    }

    init(voiceCount: Int = 10) {
        let fmt = AudioClip.format
        for _ in 0..<voiceCount {
            let p = AVAudioPlayerNode()
            let v = AVAudioUnitVarispeed()
            engine.attach(p)
            engine.attach(v)
            engine.connect(p, to: v, format: fmt)
            engine.connect(v, to: engine.mainMixerNode, format: fmt)
            voices.append((p, v))
        }
    }

    func start() {
        if !configured {
            let s = AVAudioSession.sharedInstance()
            // Mix with whatever else is playing (music, podcasts) instead of stopping it.
            try? s.setCategory(.playback, options: [.mixWithOthers])
            try? s.setPreferredIOBufferDuration(0.005)
            try? s.setActive(true)
            configured = true
        }
        if !engine.isRunning {
            engine.prepare()
            try? engine.start()
        }
    }

    func stop() {
        voices.forEach { $0.player.stop() }
        engine.stop()
    }

    func play(_ buffer: AVAudioPCMBuffer, gain: Float = 1, rate: Float = 1) {
        if !engine.isRunning { start() }
        guard engine.isRunning else { return }
        let v = voices[nextVoice]
        nextVoice = (nextVoice + 1) % voices.count
        v.player.stop()
        v.speed.rate = max(0.25, min(4, rate))
        v.player.volume = max(0, min(1, gain))
        v.player.scheduleBuffer(buffer, at: nil, options: [], completionHandler: nil)
        v.player.play()
    }
}

/// Turns key presses into sounds from the active pack: picks a variant
/// (never the same twice in a row) and adds subtle pitch/loudness jitter.
final class KeySounds {
    let engine = SoundEngine()
    private(set) var pack: LoadedPack?
    private var pressed = Set<String>()
    private var lastVariant: [String: Int] = [:]
    private let loadQueue = DispatchQueue(label: "keytone.packload", qos: .userInitiated)
    var enabled = true

    func load(_ info: PackInfo, completion: (() -> Void)? = nil) {
        loadQueue.async { [weak self] in
            let loaded = LoadedPack(info: info, format: AudioClip.format)
            DispatchQueue.main.async {
                guard let self else { return }
                self.pack = loaded
                self.lastVariant = [:]
                completion?()
            }
        }
    }

    func keyDown(_ code: String) {
        pressed.insert(code)
        play(code, .down)
    }

    func keyUp(_ code: String) {
        guard pressed.remove(code) != nil else { return }
        play(code, .up)
    }

    func play(_ code: String, _ dir: KeyDir) {
        guard enabled, let pack else { return }
        let options = pack.candidates(for: code, dir)
        guard !options.isEmpty else { return }
        let key = code + (dir == .down ? "↓" : "↑")
        var i = Int.random(in: 0..<options.count)
        if options.count > 1, i == lastVariant[key] {
            i = (i + 1 + Int.random(in: 0..<(options.count - 1))) % options.count
        }
        lastVariant[key] = i
        let r = pack.meta.random
        let cents = Double.random(in: -1...1) * max(0, r.pitchCents)
        let db = Double.random(in: -1...1) * max(0, r.gainDb)
        let rate = Float(pow(2.0, cents / 1200))
        let gain = pack.playerGain * Float(pow(10.0, db / 20))
        engine.play(options[i], gain: gain, rate: rate)
    }
}
