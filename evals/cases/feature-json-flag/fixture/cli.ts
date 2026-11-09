// Greets the name given as the first argument: `bun cli.ts acme` prints "Hello, acme!".
const who = process.argv[2] ?? "world";
console.log(`Hello, ${who}!`);
