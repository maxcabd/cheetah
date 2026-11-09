import UIKit

final class RustViewController: UIViewController {
    private var host: RustHost?
    private var keyboardObserver: NSObjectProtocol?

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = UIColor(hex: "#313338")

        let container = UIView()
        container.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(container)

        NSLayoutConstraint.activate([
            container.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor),
            container.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            container.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            container.bottomAnchor.constraint(equalTo: view.keyboardLayoutGuide.topAnchor),
        ])

        host = RustHost(container: container)

        keyboardObserver = NotificationCenter.default.addObserver(
            forName: UIResponder.keyboardWillChangeFrameNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            self?.view.layoutIfNeeded()
            self?.resizeHost()
        }
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        resizeHost()
    }

    private func resizeHost() {
        guard let host else {
            return
        }
        let size = host.containerSize
        if size.width > 0, size.height > 0 {
            host.resize(to: size)
        }
    }

    deinit {
        if let keyboardObserver {
            NotificationCenter.default.removeObserver(keyboardObserver)
        }
        host?.dispose()
    }
}
