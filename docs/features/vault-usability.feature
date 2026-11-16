Feature: Vault usability

  Background:
    Given an unlocked vault with entries:
      | project | environment | name         | notes        |
      | myapp   | dev         | DATABASE_URL | primary db   |
      | myapp   | prod        | STRIPE_KEY   |              |
      | blog    | dev         | SMTP_TOKEN   | mail relay   |

  # Entry search

  Scenario: An empty query shows the selected environment
    Given "myapp/dev" is selected
    Then the entry list shows "DATABASE_URL" only

  Scenario Outline: A query matches across environments, ignoring case
    When I search for "<query>"
    Then the entry list shows <matches>

    Examples:
      | query  | matches                          | matched on  |
      | stripe | "STRIPE_KEY" labelled myapp/prod | name        |
      | RELAY  | "SMTP_TOKEN" labelled blog/dev   | notes       |
      | blog   | "SMTP_TOKEN" labelled blog/dev   | project     |
      | prod   | "STRIPE_KEY" labelled myapp/prod | environment |

  Scenario: Choosing a match selects its environment and entry
    Given "myapp/dev" is selected
    When I search for "smtp" and choose "SMTP_TOKEN"
    Then "blog/dev" is selected and the detail pane shows "SMTP_TOKEN"

  Scenario: No match
    When I search for "nothing-like-this"
    Then I see "No entries match."

  Scenario: Escape clears the search
    Given I searched for "stripe"
    When I press Escape in the search field
    Then the query is empty and the selected environment's entries are shown

  # Generated value

  Scenario: Generate fills the value field
    Given I am creating an entry
    When I press Generate
    Then the value field holds 32 characters from A-Z a-z 0-9
    And pressing Generate again gives a different value

  # Multi-line value

  Scenario: Inner line breaks are kept
    When I save an entry with the value "line1\nline2"
    Then the stored value is "line1\nline2"

  Scenario: Trailing line breaks are removed
    When I save an entry with the value "sk-abc\r\n\n"
    Then the stored value is "sk-abc"

  Scenario: A value of only line breaks is not a value
    Given I am creating an entry
    When the value field holds "\n\n"
    Then Save is disabled

  Scenario: Reveal shows line breaks
    Given an entry whose value is "line1\nline2"
    When I reveal it
    Then the two lines are shown on separate lines

  # Shortcuts

  Scenario: Ctrl+F focuses search from any tab
    Given the Settings tab is open
    When I press Ctrl+F (or Cmd+F)
    Then the Vault tab is open and the search field has focus

  Scenario: Ctrl+L locks
    When I press Ctrl+L (or Cmd+L)
    Then the vault is locked and the unlock screen is shown

  Scenario: Shortcuts do nothing while locked
    Given the vault is locked
    When I press Ctrl+L or Ctrl+F
    Then nothing happens
