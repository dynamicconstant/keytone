import UIKit

/// One key. The touch area fills its whole slot (no dead zones between keys);
/// the visible cap is inset inside it.
final class KeyButton: UIControl {
    enum Style { case character, function, accent }

    let cap = UIView()
    let label = UILabel()
    let style: Style
    var onDown: (() -> Void)?
    var onUp: (() -> Void)?
    var onCancel: (() -> Void)?

    init(title: String? = nil, symbol: String? = nil, style: Style) {
        self.style = style
        super.init(frame: .zero)
        isMultipleTouchEnabled = true
        isExclusiveTouch = false

        cap.isUserInteractionEnabled = false
        cap.layer.cornerRadius = 5
        cap.layer.shadowColor = UIColor.black.cgColor
        cap.layer.shadowOpacity = 0.3
        cap.layer.shadowOffset = CGSize(width: 0, height: 1)
        cap.layer.shadowRadius = 0
        cap.translatesAutoresizingMaskIntoConstraints = false
        addSubview(cap)
        NSLayoutConstraint.activate([
            cap.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 3),
            cap.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -3),
            cap.topAnchor.constraint(equalTo: topAnchor, constant: 5),
            cap.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -5),
        ])

        label.isUserInteractionEnabled = false
        label.textAlignment = .center
        label.adjustsFontSizeToFitWidth = true
        label.minimumScaleFactor = 0.6
        label.translatesAutoresizingMaskIntoConstraints = false
        cap.addSubview(label)
        NSLayoutConstraint.activate([
            label.centerXAnchor.constraint(equalTo: cap.centerXAnchor),
            label.centerYAnchor.constraint(equalTo: cap.centerYAnchor),
            label.leadingAnchor.constraint(greaterThanOrEqualTo: cap.leadingAnchor, constant: 2),
            label.trailingAnchor.constraint(lessThanOrEqualTo: cap.trailingAnchor, constant: -2),
        ])
        if let symbol {
            let img = NSTextAttachment()
            img.image = UIImage(systemName: symbol, withConfiguration: UIImage.SymbolConfiguration(pointSize: 17, weight: .regular))?
                .withTintColor(.label, renderingMode: .alwaysOriginal)
            label.attributedText = NSAttributedString(attachment: img)
        } else {
            setTitle(title ?? "")
        }

        addTarget(self, action: #selector(down), for: .touchDown)
        addTarget(self, action: #selector(up), for: .touchUpInside)
        addTarget(self, action: #selector(cancel), for: [.touchUpOutside, .touchCancel])
        refreshColors()
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    func setTitle(_ t: String) {
        label.text = t
        label.font = style == .character && t.count == 1
            ? .systemFont(ofSize: 23, weight: .regular)
            : .systemFont(ofSize: 16, weight: .regular)
    }

    func setSymbol(_ name: String) {
        let img = NSTextAttachment()
        img.image = UIImage(systemName: name, withConfiguration: UIImage.SymbolConfiguration(pointSize: 17, weight: .regular))?
            .withTintColor(style == .accent ? .white : .label, renderingMode: .alwaysOriginal)
        label.attributedText = NSAttributedString(attachment: img)
    }

    override var isHighlighted: Bool {
        didSet { refreshColors() }
    }

    override func traitCollectionDidChange(_ previous: UITraitCollection?) {
        super.traitCollectionDidChange(previous)
        refreshColors()
    }

    func refreshColors() {
        let dark = traitCollection.userInterfaceStyle == .dark
        let charBG = dark ? UIColor(white: 0.42, alpha: 1) : .white
        let fnBG = dark ? UIColor(white: 0.27, alpha: 1) : UIColor(red: 0.67, green: 0.69, blue: 0.73, alpha: 1)
        switch style {
        case .character:
            cap.backgroundColor = isHighlighted ? fnBG : charBG
            label.textColor = .label
        case .function:
            cap.backgroundColor = isHighlighted ? charBG : fnBG
            label.textColor = .label
        case .accent:
            cap.backgroundColor = isHighlighted ? fnBG : .systemBlue
            label.textColor = isHighlighted ? .label : .white
        }
    }

    @objc private func down() { onDown?() }
    @objc private func up() { onUp?() }
    @objc private func cancel() { onCancel?() }
}
