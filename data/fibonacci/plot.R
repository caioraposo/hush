memo <- read.csv(file = "fib-memo-results.csv")
no_memo <- read.csv(file = "fib-no-memo-results.csv")

memo$time = memo$time / 1000

plot(no_memo$n, no_memo$time, xlab = "n", ylab = "Time (seconds)", pch = 19, frame = FALSE, col = "blue")
points(memo$n, memo$time, pch = 19, col = "red")

legend(2, 140, legend=c("W/o memoization", "With memoization"), pch=c(19, 19), col=c("blue", "red"))

