# Payment Engine

### Dangerous stuff
- no thread safe, I've deliberately choose to use only single-threaded mode for this exercise
- I'm using log::warn level for some errors so by default it won't print it to the stdout. Since we're going to utilize stdout for CSV data output, I didn't want to pollute it with the error logs (the config for the environment that you're going to use is not specified, so it is not clear which log level would be printed out)

### Assumptions
- It is not specified what should happen when the account is frozen (locked?) but I've assumed that deposits can be still made but withdrawals are blocked
- Also, the specification didn't include information on what should happen to dispute - resolve - chargeback for withdrawals, but I've assumed this case also should be possible. Although, in some cases, it is possible to get negative funds. Similar cases can happen in the real world, e.g. in the situation of account overdraft, so I assumed it should be possible
