import init, { __generated_entrance_point } from './pkg/jaime.js';
// @ts-ignore
async function main() {
  await init();
  __generated_entrance_point();
}

main().catch((_) => {});