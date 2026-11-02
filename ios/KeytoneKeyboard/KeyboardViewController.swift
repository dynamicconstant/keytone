import UIKit

/// Keytone Keyboard: a QWERTY keyboard that plays the active sound pack on
/// every key press. Sounds fire on touch-down (feels physical); characters
/// are inserted on touch-up (like the system keyboard).
final class KeyboardViewController: UIInputViewController {
    private enum Layer { case letters, numbers, symbols }
    private enum Shift { case off, once, locked }

    private enum Kind {
        case char(String)
        case shift, backspace, space, ret, globe
        case mode(Layer, String)
    }

    private struct Spec {
        let kind: Kind
        let width: CGFloat? // in tenths of the row; nil = flexible
    }

    // MARK: state

    private let sounds = KeySounds()
    private let defaults = UserDefaults.standard
    private lazy var packs: [PackInfo] = PackLibrary.all()
    private var layer: Layer = .letters
    private var shift: Shift = .once
    private var lastShiftTap = Date.distantPast
    private var lastSpace = Date.distantPast
    private var repeatTimer: Timer?
    private var repeatStart: Timer?

    // MARK: views

    private let rowsStack = UIStackView()
    private let topBar = UIStackView()
    private let packButton = UIButton(type: .system)
    private let muteButton = UIButton(type: .system)
    private let notice = UILabel()
    private let pickerScroll = UIScrollView()
    private let pickerStack = UIStackView()
    private var letterButtons: [(KeyButton, String)] = []
    private var shiftButton: KeyButton?
    private var returnButton: KeyButton?
    private var heightConstraint: NSLayoutConstraint?

    private var activePackID: String {
        get { defaults.string(forKey: "activePack") ?? "soft-thock" }
        set { defaults.set(newValue, forKey: "activePack") }
    }

    private var muted: Bool {
        get { defaults.bool(forKey: "muted") }
        set { defaults.set(newValue, forKey: "muted") }
    }

    // MARK: lifecycle

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .clear

        let height = view.heightAnchor.constraint(equalToConstant: 268)
        height.priority = UILayoutPriority(999)
        height.isActive = true
        heightConstraint = height

        buildTopBar()

        rowsStack.axis = .vertical
        rowsStack.distribution = .fillEqually
        rowsStack.spacing = 0
        rowsStack.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(rowsStack)

        pickerScroll.isHidden = true
        pickerScroll.translatesAutoresizingMaskIntoConstraints = false
        pickerStack.axis = .vertical
        pickerStack.spacing = 6
        pickerStack.translatesAutoresizingMaskIntoConstraints = false
        pickerScroll.addSubview(pickerStack)
        view.addSubview(pickerScroll)

        NSLayoutConstraint.activate([
            topBar.topAnchor.constraint(equalTo: view.topAnchor, constant: 2),
            topBar.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 8),
            topBar.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -8),
            topBar.heightAnchor.constraint(equalToConstant: 36),

            rowsStack.topAnchor.constraint(equalTo: topBar.bottomAnchor),
            rowsStack.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 0),
            rowsStack.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: 0),
            rowsStack.bottomAnchor.constraint(equalTo: view.bottomAnchor, constant: -2),

            pickerScroll.topAnchor.constraint(equalTo: topBar.bottomAnchor),
            pickerScroll.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 8),
            pickerScroll.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -8),
            pickerScroll.bottomAnchor.constraint(equalTo: view.bottomAnchor, constant: -4),

            pickerStack.topAnchor.constraint(equalTo: pickerScroll.contentLayoutGuide.topAnchor, constant: 4),
            pickerStack.leadingAnchor.constraint(equalTo: pickerScroll.contentLayoutGuide.leadingAnchor),
            pickerStack.trailingAnchor.constraint(equalTo: pickerScroll.contentLayoutGuide.trailingAnchor),
            pickerStack.bottomAnchor.constraint(equalTo: pickerScroll.contentLayoutGuide.bottomAnchor, constant: -4),
            pickerStack.widthAnchor.constraint(equalTo: pickerScroll.frameLayoutGuide.widthAnchor),
        ])

        sounds.enabled = !muted
        buildRows()
        updateTopBar()
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        sounds.engine.start()
        if sounds.pack?.id != activePackID { loadActivePack() }
        updateAutoShift()
        updateReturnKey()
        updateTopBar()
    }

    override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)
        stopRepeat()
        sounds.engine.stop()
    }

    override func textDidChange(_ textInput: UITextInput?) {
        super.textDidChange(textInput)
        updateAutoShift()
        updateReturnKey()
    }

    // MARK: top bar & pack picker

    private func buildTopBar() {
        topBar.axis = .horizontal
        topBar.alignment = .center
        topBar.spacing = 8
        topBar.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(topBar)

        packButton.titleLabel?.font = .systemFont(ofSize: 15, weight: .semibold)
        packButton.contentHorizontalAlignment = .leading
        packButton.addTarget(self, action: #selector(togglePicker), for: .touchUpInside)

        notice.font = .systemFont(ofSize: 11)
        notice.textColor = .systemOrange
        notice.numberOfLines = 2
        notice.text = "Sounds need Full Access: Settings › Keytone › Keyboards"

        muteButton.addTarget(self, action: #selector(toggleMute), for: .touchUpInside)
        muteButton.setContentHuggingPriority(.required, for: .horizontal)

        let spacer = UIView()
        spacer.setContentHuggingPriority(.defaultLow, for: .horizontal)
        packButton.setContentHuggingPriority(.required, for: .horizontal)
        [packButton, notice, spacer, muteButton].forEach(topBar.addArrangedSubview)
    }

    private func updateTopBar() {
        let name = packs.first(where: { $0.id == activePackID })?.name ?? "Pick a pack"
        let title = pickerScroll.isHidden ? "♫ \(name) ▾" : "♫ \(name) ▴"
        packButton.setTitle(title, for: .normal)
        muteButton.setImage(UIImage(systemName: muted ? "speaker.slash.fill" : "speaker.wave.2.fill"), for: .normal)
        notice.isHidden = hasFullAccess
    }

    @objc private func togglePicker() {
        if pickerScroll.isHidden { rebuildPicker() }
        pickerScroll.isHidden.toggle()
        rowsStack.isHidden = !pickerScroll.isHidden
        updateTopBar()
    }

    private func rebuildPicker() {
        pickerStack.arrangedSubviews.forEach { $0.removeFromSuperview() }
        // Two packs per row.
        var row: UIStackView?
        for (i, p) in packs.enumerated() {
            if i % 2 == 0 {
                let r = UIStackView()
                r.axis = .horizontal
                r.distribution = .fillEqually
                r.spacing = 6
                pickerStack.addArrangedSubview(r)
                row = r
            }
            var cfg = UIButton.Configuration.filled()
            cfg.title = p.name
            cfg.baseBackgroundColor = p.id == activePackID ? .systemBlue : UIColor.secondarySystemFill
            cfg.baseForegroundColor = p.id == activePackID ? .white : .label
            cfg.cornerStyle = .medium
            cfg.titleLineBreakMode = .byTruncatingTail
            let b = UIButton(configuration: cfg)
            b.heightAnchor.constraint(equalToConstant: 40).isActive = true
            b.addAction(UIAction { [weak self] _ in self?.choosePack(p) }, for: .touchUpInside)
            row?.addArrangedSubview(b)
        }
        if packs.count % 2 == 1 { row?.addArrangedSubview(UIView()) }
    }

    private func choosePack(_ p: PackInfo) {
        activePackID = p.id
        togglePicker()
        loadActivePack(preview: true)
    }

    private func loadActivePack(preview: Bool = false) {
        guard let info = packs.first(where: { $0.id == activePackID }) ?? packs.first else { return }
        sounds.load(info) { [weak self] in
            if preview { self?.sounds.play("KeyA", .down) }
        }
        updateTopBar()
    }

    @objc private func toggleMute() {
        muted.toggle()
        sounds.enabled = !muted
        updateTopBar()
    }

    // MARK: layout

    private func specs(for layer: Layer) -> [[Spec]] {
        func chars(_ s: String, _ w: CGFloat = 1) -> [Spec] { s.map { Spec(kind: .char(String($0)), width: w) } }
        let globe: [Spec] = needsInputModeSwitchKey ? [Spec(kind: .globe, width: 1.25)] : []
        switch layer {
        case .letters:
            return [
                chars("qwertyuiop"),
                [Spec(kind: .char(""), width: 0.5)] + chars("asdfghjkl") + [Spec(kind: .char(""), width: 0.5)],
                [Spec(kind: .shift, width: 1.5)] + chars("zxcvbnm") + [Spec(kind: .backspace, width: 1.5)],
                [Spec(kind: .mode(.numbers, "123"), width: 1.25)] + globe + [Spec(kind: .space, width: nil), Spec(kind: .ret, width: 2.5)],
            ]
        case .numbers:
            return [
                chars("1234567890"),
                chars("-/:;()$&@\""),
                [Spec(kind: .mode(.symbols, "#+="), width: 1.5)] + chars(".,?!'", 1.4) + [Spec(kind: .backspace, width: 1.5)],
                [Spec(kind: .mode(.letters, "ABC"), width: 1.25)] + globe + [Spec(kind: .space, width: nil), Spec(kind: .ret, width: 2.5)],
            ]
        case .symbols:
            return [
                chars("[]{}#%^*+="),
                chars("_\\|~<>€£¥•"),
                [Spec(kind: .mode(.numbers, "123"), width: 1.5)] + chars(".,?!'", 1.4) + [Spec(kind: .backspace, width: 1.5)],
                [Spec(kind: .mode(.letters, "ABC"), width: 1.25)] + globe + [Spec(kind: .space, width: nil), Spec(kind: .ret, width: 2.5)],
            ]
        }
    }

    private func buildRows() {
        rowsStack.arrangedSubviews.forEach { $0.removeFromSuperview() }
        letterButtons = []
        shiftButton = nil
        returnButton = nil

        for rowSpecs in specs(for: layer) {
            let row = UIStackView()
            row.axis = .horizontal
            row.distribution = .fill
            row.spacing = 0
            rowsStack.addArrangedSubview(row)
            for spec in rowSpecs {
                let v = makeKey(spec.kind)
                row.addArrangedSubview(v)
                if let w = spec.width {
                    let c = v.widthAnchor.constraint(equalTo: row.widthAnchor, multiplier: w / 10)
                    c.priority = UILayoutPriority(999)
                    c.isActive = true
                } else {
                    v.setContentHuggingPriority(.defaultLow, for: .horizontal)
                }
            }
        }
        updateLetterCase()
        updateReturnKey()
    }

    private func makeKey(_ kind: Kind) -> UIView {
        switch kind {
        case .char(let c) where c.isEmpty:
            return UIView() // half-key spacer on the home row
        case .char(let c):
            let b = KeyButton(title: c, style: .character)
            let code = KeyCodes.code(for: c)
            b.onDown = { [weak self] in self?.sounds.keyDown(code) }
            b.onUp = { [weak self] in
                guard let self else { return }
                self.insert(self.shifted(c))
                self.sounds.keyUp(code)
            }
            b.onCancel = { [weak self] in self?.sounds.keyUp(code) }
            if layer == .letters { letterButtons.append((b, c)) }
            return b
        case .shift:
            let b = KeyButton(symbol: "shift", style: .function)
            b.onDown = { [weak self] in self?.sounds.keyDown("ShiftLeft") }
            b.onUp = { [weak self] in
                self?.tapShift()
                self?.sounds.keyUp("ShiftLeft")
            }
            b.onCancel = { [weak self] in self?.sounds.keyUp("ShiftLeft") }
            shiftButton = b
            return b
        case .backspace:
            let b = KeyButton(symbol: "delete.left", style: .function)
            b.onDown = { [weak self] in
                guard let self else { return }
                self.sounds.keyDown("Backspace")
                self.deleteBackward()
                self.startRepeat()
            }
            b.onUp = { [weak self] in
                self?.stopRepeat()
                self?.sounds.keyUp("Backspace")
            }
            b.onCancel = b.onUp
            return b
        case .space:
            let b = KeyButton(title: "space", style: .character)
            b.label.font = .systemFont(ofSize: 16)
            b.onDown = { [weak self] in self?.sounds.keyDown("Space") }
            b.onUp = { [weak self] in
                self?.tapSpace()
                self?.sounds.keyUp("Space")
            }
            b.onCancel = { [weak self] in self?.sounds.keyUp("Space") }
            return b
        case .ret:
            let b = KeyButton(title: "return", style: .function)
            b.onDown = { [weak self] in self?.sounds.keyDown("Enter") }
            b.onUp = { [weak self] in
                self?.insert("\n")
                self?.sounds.keyUp("Enter")
            }
            b.onCancel = { [weak self] in self?.sounds.keyUp("Enter") }
            returnButton = b
            return b
        case .mode(let target, let title):
            let b = KeyButton(title: title, style: .function)
            b.onDown = { [weak self] in self?.sounds.keyDown("AltLeft") }
            b.onUp = { [weak self] in
                self?.sounds.keyUp("AltLeft")
                self?.switchLayer(target)
            }
            b.onCancel = { [weak self] in self?.sounds.keyUp("AltLeft") }
            return b
        case .globe:
            let b = KeyButton(symbol: "globe", style: .function)
            b.addTarget(self, action: #selector(handleInputModeList(from:with:)), for: .allTouchEvents)
            return b
        }
    }

    // MARK: typing

    private func shifted(_ c: String) -> String {
        guard layer == .letters, shift != .off else { return c }
        return c.uppercased()
    }

    private func insert(_ text: String) {
        textDocumentProxy.insertText(text)
        if shift == .once { shift = .off }
        updateAutoShift()
    }

    private func deleteBackward() {
        textDocumentProxy.deleteBackward()
        updateAutoShift()
    }

    private func tapSpace() {
        let now = Date()
        let before = textDocumentProxy.documentContextBeforeInput ?? ""
        // Double-space inserts a period, like the system keyboard.
        if now.timeIntervalSince(lastSpace) < 0.45, before.hasSuffix(" "),
           let prev = before.dropLast().last, prev.isLetter || prev.isNumber {
            textDocumentProxy.deleteBackward()
            textDocumentProxy.insertText(". ")
            lastSpace = .distantPast
        } else {
            textDocumentProxy.insertText(" ")
            lastSpace = now
        }
        if layer != .letters { switchLayer(.letters) }
        if shift == .once { shift = .off }
        updateAutoShift()
    }

    private func tapShift() {
        let now = Date()
        if now.timeIntervalSince(lastShiftTap) < 0.3 {
            shift = .locked
        } else {
            shift = shift == .off ? .once : .off
        }
        lastShiftTap = now
        updateLetterCase()
    }

    private func switchLayer(_ l: Layer) {
        layer = l
        buildRows()
        updateAutoShift()
    }

    private func startRepeat() {
        stopRepeat()
        repeatStart = Timer.scheduledTimer(withTimeInterval: 0.45, repeats: false) { [weak self] _ in
            self?.repeatTimer = Timer.scheduledTimer(withTimeInterval: 0.08, repeats: true) { [weak self] _ in
                self?.deleteBackward()
            }
        }
    }

    private func stopRepeat() {
        repeatStart?.invalidate()
        repeatTimer?.invalidate()
        repeatStart = nil
        repeatTimer = nil
    }

    // MARK: auto-capitalization & labels

    private func updateAutoShift() {
        guard shift != .locked else { return updateLetterCase() }
        let proxy = textDocumentProxy
        let mode = proxy.autocapitalizationType ?? .sentences
        let before = proxy.documentContextBeforeInput ?? ""
        let shouldCap: Bool
        switch mode {
        case .none:
            shouldCap = false
        case .allCharacters:
            shouldCap = true
        case .words:
            shouldCap = before.isEmpty || before.last?.isWhitespace == true
        default: // .sentences
            let trimmed = before.trimmingCharacters(in: .whitespaces)
            shouldCap = trimmed.isEmpty || before.hasSuffix("\n")
                || ((trimmed.hasSuffix(".") || trimmed.hasSuffix("!") || trimmed.hasSuffix("?")) && before.last == " ")
        }
        shift = shouldCap ? .once : .off
        updateLetterCase()
    }

    private func updateLetterCase() {
        for (b, c) in letterButtons {
            b.setTitle(shift == .off ? c : c.uppercased())
        }
        guard let s = shiftButton else { return }
        switch shift {
        case .off: s.setSymbol("shift")
        case .once: s.setSymbol("shift.fill")
        case .locked: s.setSymbol("capslock.fill")
        }
    }

    private func updateReturnKey() {
        guard let r = returnButton else { return }
        let title: String
        switch textDocumentProxy.returnKeyType ?? .default {
        case .go: title = "go"
        case .google, .search, .yahoo: title = "search"
        case .join: title = "join"
        case .next: title = "next"
        case .route: title = "route"
        case .send: title = "send"
        case .done: title = "done"
        case .emergencyCall: title = "SOS"
        case .continue: title = "continue"
        default: title = "return"
        }
        r.setTitle(title)
    }
}
