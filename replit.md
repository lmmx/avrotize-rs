# HasseMap Library Project

## Overview
A Rust library crate that provides an order-preserving hash map data structure called `HasseMap`. This data structure maintains insertion order while providing fast hash-based lookups, similar to the ordermap crate's OrderMap but with its own implementation. The library is designed as a reusable crate with a clean API and comprehensive documentation.

## User Preferences
Preferred communication style: Simple, everyday language.

## Project Architecture

### Library Structure
- **Library Crate**: Standard Rust library project with `src/lib.rs` as the entry point
- **Module Organization**: 
  - `src/lib.rs` - Library root that exports the HasseMap data structure
  - `src/hasse_map.rs` - Contains the main HasseMap implementation
- **Package Configuration**: `Cargo.toml` with proper metadata for publishing

### HasseMap Implementation
- **Core Data Structure**: 
  - `HashMap<K, V>` for O(1) key-value storage
  - `Vec<K>` for maintaining computed linear order
  - `HashMap<K, HashSet<K>>` for tracking partial order constraints
- **Key Features**:
  - **Partial Order Support**: Batch updates establish ordering constraints between keys
  - **Topological Sorting**: Maintains linear extensions of partial orders
  - **Deterministic Behavior**: Sorts batch keys before constraint creation for consistent results
  - Fast hash-based key lookups
  - Standard map operations (insert, get, remove, contains_key)
  - Comprehensive iterator support (keys, values, key-value pairs)
  - Mutable and immutable access patterns

### API Design
- **Constructor Methods**: `new()` and `with_capacity()`
- **Core Operations**: insert, get, get_mut, remove, contains_key
- **Batch Operations**: `batch_update()` for establishing partial order constraints
- **Utility Methods**: len, is_empty, clear
- **Iterator Support**: keys(), values(), iter(), iter_mut() (all respect computed order)
- **Trait Implementations**: Debug, PartialEq, Eq, Default, IntoIterator
- **Constraints**: Keys must implement `Hash + Eq + Clone + Ord`

### Testing Strategy
- **Unit Tests**: Comprehensive test suite covering all functionality
- **Documentation Tests**: All public methods include working examples
- **Test Coverage**: 12 unit tests + 15 documentation tests, all passing

## External Dependencies
- **Standard Library Only**: Uses `std::collections::HashMap` and `std::vec::Vec`
- **No External Crates**: Self-contained implementation with no third-party dependencies
- **Development Tools**: Uses Cargo's built-in testing framework

## Recent Changes
- ✅ Enhanced HasseMap to support partial order constraints from batch updates
- ✅ Added `batch_update()` method for establishing partial orders between keys
- ✅ Implemented deterministic topological sorting for linear extensions
- ✅ Added `K: Ord` constraint for stable ordering behavior
- ✅ Updated comprehensive test suite - all 12 unit tests + 16 documentation tests passing
- ✅ Proper Rust library crate structure maintained

## Performance Characteristics
- **Lookup Time**: O(1) average case for key-based access
- **Single Insertion**: O(1) average case for new keys
- **Batch Update**: O(V + E) where V = number of keys, E = number of constraints (topological sort)
- **Iteration Time**: O(n) following computed linear order
- **Memory Usage**: HashMap + Vec + constraint graph storage
- **Space Complexity**: O(V + E) for storing partial order constraints

## Partial Order Examples
```rust
// Batch 1: {b:1, c:1} → establishes b < c
// Batch 2: {a:2, b:2} → establishes a < b  
// Result: a < b < c (linear extension respects both constraints)
```