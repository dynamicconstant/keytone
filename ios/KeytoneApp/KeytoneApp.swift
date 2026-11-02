import SwiftUI
import UIKit

@main
struct KeytoneApp: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
        }
    }
}

final class Previewer: ObservableObject {
    private let sounds = KeySounds()
    @Published var loading: String?

    /// Play a short demo of a pack: a few keys, then its Space and Enter sounds.
    func preview(_ p: PackInfo) {
        loading = p.id
        sounds.engine.start()
        sounds.load(p) { [weak self] in
            guard let self else { return }
            self.loading = nil
            let demo: [(String, Double)] = [("KeyH", 0), ("KeyE", 0.16), ("KeyY", 0.32), ("Space", 0.55), ("Enter", 1.0)]
            for (code, at) in demo {
                DispatchQueue.main.asyncAfter(deadline: .now() + at) { self.sounds.play(code, .down) }
            }
        }
    }
}

struct ContentView: View {
    @StateObject private var previewer = Previewer()
    @State private var text = ""
    private let packs = PackLibrary.all()

    var body: some View {
        NavigationStack {
            List {
                Section {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Make every keystroke sound like anything you want.")
                            .font(.headline)
                        Text("Keytone is a keyboard. Turn it on once, then switch to it with the 🌐 key in any app.")
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                    }
                    .padding(.vertical, 4)
                }

                Section("Set up (one time)") {
                    Step(n: 1, text: "Tap **Open Settings** below, then **Keyboards**.")
                    Step(n: 2, text: "Turn on **Keytone**.")
                    Step(n: 3, text: "Turn on **Allow Full Access**. iOS only lets a keyboard play its own sounds with Full Access.")
                    Step(n: 4, text: "In any app, hold 🌐 and pick **Keytone**.")
                    Button {
                        if let url = URL(string: UIApplication.openSettingsURLString) {
                            UIApplication.shared.open(url)
                        }
                    } label: {
                        Label("Open Settings", systemImage: "gear")
                    }
                }

                Section("Try it") {
                    TextField("Switch to Keytone and type here…", text: $text, axis: .vertical)
                        .lineLimit(2...5)
                    Text("Choose a pack from the ♫ button at the top of the keyboard.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }

                Section("Sound packs (\(packs.count))") {
                    ForEach(packs) { p in
                        Button {
                            previewer.preview(p)
                        } label: {
                            HStack(spacing: 12) {
                                Image(systemName: previewer.loading == p.id ? "hourglass" : "play.circle.fill")
                                    .font(.title2)
                                    .foregroundStyle(.tint)
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(p.name).foregroundStyle(.primary)
                                    Text(p.description)
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                        .lineLimit(2)
                                }
                            }
                        }
                    }
                }

                Section("Privacy") {
                    Text("Full Access makes iOS warn that a keyboard *could* send what you type. Keytone never does: it only plays a sound for the key you pressed, never stores or sends your typing, and makes no network requests.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
            }
            .navigationTitle("Keytone")
        }
    }
}

private struct Step: View {
    let n: Int
    let text: LocalizedStringKey

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Text("\(n)")
                .font(.caption.bold())
                .frame(width: 20, height: 20)
                .background(Circle().fill(Color.accentColor.opacity(0.2)))
            Text(text)
        }
    }
}
