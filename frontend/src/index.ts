import { faRobot } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { createElement } from 'react';
import { Extension, type ExtensionContext } from 'shared';
import { chatShells } from './lib/display.ts';
import AccountKeyPage from './pages/AccountKeyPage.tsx';
import AdminAssistantPage from './pages/AdminAssistantPage.tsx';
import AdminChatPopup from './pages/AdminChatPopup.tsx';
import ServerAssistantPage from './pages/ServerAssistantPage.tsx';
import ServerChatPopup from './pages/ServerChatPopup.tsx';
import SettingsPage from './pages/SettingsPage.tsx';
import { getExtTranslations } from './translations.ts';

class DevCaloptreyxAiAssistantExtension extends Extension {
  public cardIcon = createElement(FontAwesomeIcon, { icon: faRobot });
  public cardConfigurationPage: React.FC | null = SettingsPage;

  public initialize(ctx: ExtensionContext): void {
    // The backend renders the "Chat display" setting into index.html, so it is known before the
    // sidebars are built. A changed setting applies after the next page load.
    const shells = chatShells(this.getIndexVariable('chat_display'));
    const pageShown = () => shells.page;

    ctx.extensionRegistry.routes
      .addServerRoute({
        name: () => getExtTranslations().t('title', {}),
        icon: faRobot,
        path: '/ai-assistant',
        element: ServerAssistantPage,
        permission: 'ai-assistant.chat',
        filter: pageShown,
      })
      .addAdminRoute({
        name: () => getExtTranslations().t('title', {}),
        icon: faRobot,
        path: '/ai-assistant',
        element: AdminAssistantPage,
        permission: 'ai-assistant.chat',
        filter: pageShown,
      })
      .addAccountRoute({
        name: () => getExtTranslations().t('account.title', {}),
        icon: faRobot,
        path: '/ai-assistant',
        element: AccountKeyPage,
      });

    if (shells.popup) {
      ctx.extensionRegistry.pages.server.appendComponent(ServerChatPopup);
      ctx.extensionRegistry.pages.admin.appendComponent(AdminChatPopup);
    }

    ctx.extensionRegistry.enterPermissionIcons((icons) =>
      icons
        .addServerPermissionIcon('ai-assistant', createElement(FontAwesomeIcon, { icon: faRobot }))
        .addAdminPermissionIcon('ai-assistant', createElement(FontAwesomeIcon, { icon: faRobot }))
        .addUserPermissionIcon('ai-assistant', createElement(FontAwesomeIcon, { icon: faRobot })),
    );
  }
}

export default new DevCaloptreyxAiAssistantExtension();
