import SwiftUI

@MainActor
func featureBinding<State: AnyObject, Value>(
    _ state: State,
    _ keyPath: ReferenceWritableKeyPath<State, Value>
) -> Binding<Value> {
    Binding(
        get: { state[keyPath: keyPath] },
        set: { state[keyPath: keyPath] = $0 }
    )
}
