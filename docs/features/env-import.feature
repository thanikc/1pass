Feature: Env file import

  Background:
    Given an unlocked vault with environment "myapp/dev"
    And "myapp/dev" has an entry "DATABASE_URL" with notes "primary db"

  Scenario: Candidates are listed without values
    Given a .env file containing:
      """
      STRIPE_SECRET=sk_live_abc
      LOG_LEVEL=debug
      API_URL=op://myapp/dev/API_URL
      EMPTY=
      DATABASE_URL=postgres://u:p@host/db
      """
    When I pick that file to import into "myapp/dev"
    Then the candidates are:
      | name          | status    | replaces | preselected |
      | STRIPE_SECRET | secret    | no       | yes         |
      | LOG_LEVEL     | literal   | no       | no          |
      | API_URL       | reference | no       | no          |
      | EMPTY         | empty     | no       | no          |
      | DATABASE_URL  | literal   | yes      | no          |
    And no value from the file is sent to the frontend

  Scenario: Chosen candidates become entries
    When I import "STRIPE_SECRET" and "LOG_LEVEL"
    Then "myapp/dev" has entries "STRIPE_SECRET" and "LOG_LEVEL" with the values from the file
    And I am shown the lines:
      """
      LOG_LEVEL=op://myapp/dev/LOG_LEVEL
      STRIPE_SECRET=op://myapp/dev/STRIPE_SECRET
      """
    And the .env file is unchanged

  Scenario: An existing entry is replaced only when chosen
    When I import "DATABASE_URL"
    Then the value of "DATABASE_URL" is the one from the file
    And its notes are still "primary db"

  Scenario: An existing entry that was not chosen is untouched
    When I import "STRIPE_SECRET"
    Then the value of "DATABASE_URL" is unchanged

  Scenario Outline: A name that cannot be imported stores nothing
    When I import "STRIPE_SECRET" and "<name>"
    Then the import fails
    And "myapp/dev" has no entry "STRIPE_SECRET"

    Examples:
      | name    | why                    |
      | API_URL | it is a reference      |
      | EMPTY   | its value is empty     |
      | MISSING | it is not in the file  |

  Scenario: Later duplicates win
    Given a .env file containing "TOKEN=first" then "TOKEN=second"
    When I import "TOKEN"
    Then the value of "TOKEN" is "second"

  Scenario: Cancelling the file dialog changes nothing
    When I cancel the file dialog
    Then no import dialog opens and the vault is unchanged

  Scenario Outline: Unreadable files are refused
    When I pick a file that <problem>
    Then I see an error and the vault is unchanged

    Examples:
      | problem                 |
      | has a dotenv syntax error |
      | is larger than 1 MiB    |
      | is not UTF-8            |

  Scenario: The file changed between preview and import
    Given I picked a file that contained "TOKEN"
    And "TOKEN" was removed from the file afterwards
    When I import "TOKEN"
    Then the import fails and the vault is unchanged

  Scenario: Import without a picked file
    When the frontend asks to import without a file having been picked
    Then the import fails
