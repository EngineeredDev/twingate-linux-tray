import { Command } from "@tauri-apps/plugin-shell";
import TwingateSchema from "./schemas";

export async function getResources() {
  const data = await getTwingateNetworkData();

  return data?.resources || [];
}

export async function getUserInfo() {
  const data = await getTwingateNetworkData();

  return data?.user;
}

export async function getTwingateNetworkData() {
  const result = await Command.create('twingate-notifier', ['resources']).execute();

  // TODO: proper error handling on bail out
  if (result.stderr) return;

  // TODO: proper handling if parse fails
  const parsedResult = TwingateSchema.safeParse(JSON.parse(result.stdout));

  if (parsedResult.error) {
    console.error("twingate-notifier parse failure", parsedResult.error);
    return;
  }

  console.log(parsedResult.data.resources);

  return parsedResult.data;
}
