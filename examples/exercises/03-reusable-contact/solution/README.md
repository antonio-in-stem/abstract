# Solution

`Contact` owns the shape and its `logic Contact` block owns the shared rule.
Both `Person.contact` and `Vendor.contact` use `$(Contact)`, so the compiler
validates both nested values with the same logic. The two root schemas do not
need duplicate checks.
