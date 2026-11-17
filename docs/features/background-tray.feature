Feature: Background tray

  These run in the real window and tray, so they are checked by hand
  (docs/manual-test-checklist.md), not by automated tests.

  Scenario: Closing the window keeps the app running
    Given the app is running with a tray icon
    When I close the window
    Then the window is hidden
    And `op run` can still reach the app

  Scenario: The tray opens the window
    Given the window is hidden
    When I choose Open in the tray menu, or left-click the tray icon
    Then the window is shown and focused

  Scenario: The tray locks the vault
    Given the vault is unlocked
    When I choose Lock in the tray menu
    Then the vault is locked and time-boxed approvals are gone

  Scenario: The tray quits the app
    When I choose Quit in the tray menu
    Then the app exits, the socket is removed and a copied value is cleared from the clipboard

  Scenario: A request raises the hidden window
    Given the window is hidden
    When `op run` needs an unlock or an approval
    Then the window is shown and focused

  Scenario: A second launch raises the hidden window
    Given the window is hidden
    When I start one-pass again
    Then the first instance's window is shown and focused

  Scenario: The vault still auto-locks while hidden
    Given the vault is unlocked, auto-lock is 1 minute and the window is hidden
    When 1 minute passes
    Then the vault is locked

  Scenario: Without a tray icon, closing quits
    Given the tray icon could not be created
    When I close the window
    Then the app exits
