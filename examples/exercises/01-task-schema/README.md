# Exercise 01: describe a task

Write the smallest useful task model. `Task` must have a title, a priority
from 1 through 3, and a state chosen from `todo`, `doing` or `done`. A missing
state should become `todo`.

Open `starter/data/templates/Task.abt` and
`starter/data/tasks/tasks.ab`. Fix the deliberate invalid value so the project
compiles. The solution is in `solution/`.

Check yourself: priorities `1` and `3` must work, `0` must fail, and omitting
state must emit `todo`. Do not remove the range to make the bad value pass.

<details>
<summary>Hint</summary>

constraints and defaults belong in the `.abt` schema. The `.ab` file
selects a schema and supplies an object id.

</details>
