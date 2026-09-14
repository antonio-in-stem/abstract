# Solution

The schema already states the intended enum, range and default. The solution
keeps that contract and fixes the two package values: `sent` is a declared
state and `4` is within `1..25`. The first package omits state to demonstrate
the `queued` default.
