import Foundation
import Security

/**
 `secureStore` module (Rust: `runtime::secure_store`).

 Values are Keychain generic passwords under the service
 `<bundle id>.secureStore`, readable after first unlock and never migrated to
 another device. All work runs on one serial queue so calls complete in the
 order Rust queued them; results are emitted on the main queue.
 */
final class SecureStoreApi {
    private struct KeychainError: Error, CustomStringConvertible {
        let description: String
    }

    private let queue = DispatchQueue(label: "cheetah.secure-store")
    private let service = (Bundle.main.bundleIdentifier ?? "cheetah") + ".secureStore"
    private let emit: ([String: Any]) -> Void

    init(emit: @escaping ([String: Any]) -> Void) {
        self.emit = emit
    }

    func invoke(_ method: String, args: [String: Any]) -> Bool {
        guard method == "get" || method == "set" || method == "delete" else {
            return false
        }
        let key = args["key"] as? String ?? ""
        let value = args["value"] as? String

        queue.async { [weak self] in
            guard let self else { return }
            var result: [String: Any] = ["op": method, "key": key]
            do {
                guard !key.isEmpty else { throw KeychainError(description: "missing_key") }
                switch method {
                case "get":
                    result["value"] = try self.read(key) ?? NSNull()
                case "set":
                    guard let value else { throw KeychainError(description: "missing_value") }
                    try self.write(key, value)
                default:
                    try self.remove(key)
                }
                result["ok"] = true
            } catch {
                result["ok"] = false
                result["error"] = String(describing: error)
            }
            let payload = result
            DispatchQueue.main.async { self.emit(payload) }
        }
        return true
    }

    private func baseQuery(_ key: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key,
        ]
    }

    private func read(_ key: String) throws -> String? {
        var query = baseQuery(key)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne

        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        switch status {
        case errSecSuccess:
            guard let data = item as? Data else { return nil }
            return String(data: data, encoding: .utf8)
        case errSecItemNotFound:
            return nil
        default:
            throw KeychainError(description: "keychain_\(status)")
        }
    }

    private func write(_ key: String, _ value: String) throws {
        let data = Data(value.utf8)
        let update: [String: Any] = [kSecValueData as String: data]
        let status = SecItemUpdate(baseQuery(key) as CFDictionary, update as CFDictionary)
        if status == errSecSuccess {
            return
        }
        guard status == errSecItemNotFound else {
            throw KeychainError(description: "keychain_\(status)")
        }

        var add = baseQuery(key)
        add[kSecValueData as String] = data
        add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let added = SecItemAdd(add as CFDictionary, nil)
        guard added == errSecSuccess else {
            throw KeychainError(description: "keychain_\(added)")
        }
    }

    private func remove(_ key: String) throws {
        let status = SecItemDelete(baseQuery(key) as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw KeychainError(description: "keychain_\(status)")
        }
    }
}
