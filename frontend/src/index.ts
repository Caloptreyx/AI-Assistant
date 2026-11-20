import { faRobot } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { createElement } from 'react';
import { Extension, type ExtensionContext } from 'shared';
import AdminAssistantPage from './pages/AdminAssistantPage.tsx';
import ServerAssistantPage from './pages/ServerAssistantPage.tsx';
import SettingsPage from './pages/SettingsPage.tsx';
import { getExtTranslations } from './translations.ts';

class DevCaloptreyxAiAssistantExtension extends Extension {
  public cardIcon = createElement(FontAwesomeIcon, { icon: faRobot });
  public cardConfigurationPage: React.FC | null = SettingsPage;

  public initialize(ctx: ExtensionContext): void {
    ctx.extensionRegistry.routes
      .addServerRoute({
        name: () => getExtTranslations().t('title', {}),
        icon: faRobot,
        path: '/ai-assistant',
        element: ServerAssistantPage,
        permission: 'ai-assistant.chat',
      })
      .addAdminRoute({
        name: () => getExtTranslations().t('title', {}),
        icon: faRobot,
        path: '/ai-assistant',
        element: AdminAssistantPage,
        permission: 'ai-assistant.chat',
      });

    ctx.extensionRegistry.enterPermissionIcons((icons) =>
      icons
        .addServerPermissionIcon('ai-assistant', createElement(FontAwesomeIcon, { icon: faRobot }))
        .addAdminPermissionIcon('ai-assistant', createElement(FontAwesomeIcon, { icon: faRobot })),
    );
  }
}

export default new DevCaloptreyxAiAssistantExtension();
