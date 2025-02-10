*** Settings ***
Resource            ../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Test Teardown

Test Tags           theme:cli    theme:http    theme:childdevices


*** Test Cases ***
Sanity check: No HTTP service on a child device
    Execute Command    curl http://localhost:8000/tedge/entity-store/v1/entities    exp_exit_code=7

Listing entities from a child device
    ${entities}=    Execute Command    tedge http get /tedge/entity-store/v1/entities
    Should Contain    ${entities}    device/main//
    Should Contain    ${entities}    device/${CHILD_SN}//

Updating entities from a child device
    Execute Command
    ...    tedge http post /tedge/entity-store/v1/entities '{"@topic-id": "device/${CHILD_SN}/service/watchdog", "@type": "service", "@parent": "device/${CHILD_SN}//"}'
    ${entity}=    Execute Command    tedge http get /tedge/entity-store/v1/entities/device/${CHILD_SN}/service/watchdog
    Should Contain    ${entity}    "@topic-id":"device/${CHILD_SN}/service/watchdog"
    Should Contain    ${entity}    "@parent":"device/${CHILD_SN}//"
    Should Contain    ${entity}    "@type":"service"
    Execute Command    tedge http delete /tedge/entity-store/v1/entities/device/${CHILD_SN}/service/watchdog
    Execute Command
    ...    tedge http get /tedge/entity-store/v1/entities/device/${CHILD_SN}/service/watchdog
    ...    exp_exit_code=1

Accessing c8y from a child device
    ${external_id}=    Execute Command
    ...    tedge http get /c8y/identity/externalIds/c8y_Serial/${PARENT_SN} | jq .externalId
    Should Be Equal    ${external_id}    "${PARENT_SN}"\n

Accessing file-transfer from a child device
    Execute Command    printf "source file content" >/tmp/source-file.txt
    Execute Command    tedge http put /tedge/file-transfer/target --file /tmp/source-file.txt --content-type text/plain
    ${content}=    Execute Command    tedge http get /tedge/file-transfer/target
    Should Be Equal    ${content}    source file content
    Execute Command    tedge http delete /tedge/file-transfer/target
    Execute Command    tedge http get /tedge/file-transfer/target    exp_exit_code=1

Displaying server errors
    ${error_msg}=    Execute Command
    ...    tedge http post /tedge/entity-store/v1/entities '{"@topic-id": "device/a//", "@type": "child-device", "@parent": "device/unknown//"}' 2>&1
    ...    exp_exit_code=1
    Should Contain    ${error_msg}    400 Bad Request
    Should Contain    ${error_msg}    Specified parent "device/unknown//" does not exist in the store


*** Keywords ***
Setup Child Device
    ThinEdgeIO.Set Device Context    ${CHILD_SN}
    Execute Command    sudo dpkg -i packages/tedge_*.deb

    Execute Command    sudo tedge config set http.client.host ${PARENT_IP}
    Execute Command    sudo tedge config set http.client.port 8000
    Execute Command    sudo tedge config set c8y.proxy.client.host ${PARENT_IP}
    Execute Command    sudo tedge config set c8y.proxy.client.port 8001
    Execute Command    sudo tedge config set mqtt.client.host ${PARENT_IP}
    Execute Command    sudo tedge config set mqtt.client.port 1883
    Execute Command    sudo tedge config set mqtt.topic_root te
    Execute Command    sudo tedge config set mqtt.device_topic_id "device/${CHILD_SN}//"

    # Install plugin after the default settings have been updated to prevent it from starting up as the main plugin
    Execute Command    sudo dpkg -i packages/tedge-agent*.deb
    Execute Command    sudo systemctl enable tedge-agent
    Execute Command    sudo systemctl start tedge-agent

Custom Setup
    # Parent
    ${parent_sn}=    Setup    skip_bootstrap=${True}
    Set Suite Variable    $PARENT_SN    ${parent_sn}
    Execute Command    test -f ./bootstrap.sh && ./bootstrap.sh --no-connect || true

    ${parent_ip}=    Get IP Address
    Set Suite Variable    $PARENT_IP    ${parent_ip}
    Execute Command    sudo tedge config set mqtt.external.bind.address ${PARENT_IP}
    Execute Command    sudo tedge config set mqtt.external.bind.port 1883
    Execute Command    sudo tedge config set http.bind.address ${PARENT_IP}
    Execute Command    sudo tedge config set http.bind.port 8000
    Execute Command    sudo tedge config set c8y.proxy.bind.address ${PARENT_IP}
    Execute Command    sudo tedge config set c8y.proxy.bind.port 8001

    ThinEdgeIO.Connect Mapper    c8y
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-c8y

    # Child
    ${CHILD_SN}=    Setup    skip_bootstrap=${True}
    Set Suite Variable    $CHILD_SN
    Set Suite Variable    $CHILD_XID    ${PARENT_SN}:device:${CHILD_SN}
    Setup Child Device
    Cumulocity.Device Should Exist    ${CHILD_XID}

Test Teardown
    Get Logs    name=${PARENT_SN}
    Get Logs    name=${CHILD_SN}
