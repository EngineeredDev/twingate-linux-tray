import { Menu, MenuItem, MenuItemOptions, Submenu } from "@tauri-apps/api/menu";
import { resolveResource } from "@tauri-apps/api/path";
import { TrayIcon, TrayIconOptions } from "@tauri-apps/api/tray";
import { getResources, getTwingateNetworkData, getUserInfo } from "./twingate-cli";
import { Resource } from "./schemas";

// export class TwingateTray {
//   private _tray!: TrayIcon;
//   private _menu!: Menu;
//
//   // should use TwingateTray.new() instead
//   private constructor() { }
//
//   public static async new() {
//     const tray = new TwingateTray();
//     // const user = await getUserInfo();
//
//     tray._menu = await Menu.new({ id: "twingate-tray" })
//
//     // const menu = await Menu.new({
//     //   items: [
//     //     {
//     //       id: "user",
//     //       text: user?.email || "Unknown User",
//     //     },
//     //     ...(await getResourceMenuItems())
//     //   ],
//     // });
//
//     const iconPath = await resolveResource('icons/icon.png');
//
//     const options: TrayIconOptions = {
//       icon: iconPath,
//       menu: tray._menu,
//     };
//
//     tray._tray = await TrayIcon.new(options);
//
//     return tray;
//   }
//
//   private async generateMenuItems() {
//     if (!this._menu) {
//       this._menu = await Menu.new({ id: "twingate-tray" });
//     }
//
//     const resources = await getResources();
//
//     this._tray.setMenu(this._menu);
//
//   }
//
//   private async generateResourceMenuItems() { }
// }

// class TwingateMenu extends Menu {
//   public static async new(): Promise<TwingateMenu> {
//     return await Menu.new({ id: "twingate-tray" })
//   }
//
// }

class TwingateInteractiveMenuItem extends MenuItem {
  handleAction(id: string) {

  }
  static new(opts: MenuItemOptions): Promise<MenuItem> {
    return super.new(opts);
  }
}
//
// class ResourceMenuItem extends Submenu {
//   static new(resource: Resource): Promise<Submenu> {
//     return super.new(opts);
//   }
// }

export async function getTwingateTray() {
  const menu = await getTwingateMenu();
  const iconPath = await resolveResource('icons/icon.png');

  const options = {
    icon: iconPath,
    menu,
    menuOnLeftClick: true,
  };

  const tray = await TrayIcon.new(options);


  return tray;
}

async function getTwingateMenu() {
  const data = await getTwingateNetworkData();
  const user = data?.user;

  const menu = await Menu.new({
    items: [
      {
        id: "user",
        text: user?.email || "Unknown User",
      },
      ...(await getResourceMenuItems())
    ],
  });

  return menu;
}

async function getResourceMenuItems(): Promise<MenuItemOptions[]> {

  const resources = await getResources();

  return resources.map((r) => ({
    id: r.id,
    text: r.alias || r.name,
    action: (x) => {

    }
  }))
}
