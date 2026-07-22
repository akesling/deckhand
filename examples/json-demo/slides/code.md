# syntax highlighting

````row
```rust
// classic demo fodder
fn fib(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    fib(n - 1) + fib(n - 2)
}
```
||
```python
def fib(n):
    """n-th fibonacci"""
    a, b = 0, 1
    for _ in range(n):
        a, b = b, a + b
    return a  # done
```
````

fenced code with a language tag lights up — keywords, strings,
comments, numbers. shell & data formats below ↓
