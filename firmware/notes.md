# Notes

## Pinouts

USART1 -------------------------
RXD: PB07 (also PB08)
TXD: PB06 (also PB03,04,05)
DE:  PA12 (also PA10)

GPIOs (out) --------------------
LED1: PB09 (also PC14)
LED2: PC15
IO1: PA11 (also PA09)
    -> P1.01
IO2: PA07
    -> P1.02

SPI1 ---------------------------
SCLK: PA01
    -> P1.04
MOSI: PA02
    -> P1.03
MISO: PA06
    -> P1.05
CSn:  PB00 (also PB01,02, PA08)
    -> P1.06

Bitbang I2C --------------------
PA05: SDA (BB)
PA04: SCL (BB)


Testpoint? ---------------------

LED: PA00 - Just kidding, not a SPI2 option?

## SPI notes

> SSM = 0, SSOE = 0
>
> In slave mode, the NSS pin works as a standard “chip select” input
> and the slave is selected while NSS line is at low level.

### Configuration of SPI

The configuration procedure is almost the same for master and slave. For specific mode setups, follow the dedicated sections. When a standard communication is to be initialized, perform these steps:


1. Write proper GPIO registers: Configure GPIO for MOSI, MISO and SCK pins.
2. Write to the SPI_CR1 register:
    * XXX - Configure the serial clock baud rate using the BR[2:0] bits (Note: 4).
    * Configure the CPOL and CPHA bits combination to define one of the four relationships between the data transfer and the serial clock (CPHA must be cleared in NSSP mode).
    * Select simplex or half-duplex mode by configuring RXONLY or BIDIMODE and BIDIOE (RXONLY and BIDIMODE can't be set at the same time).
    * Configure the LSBFIRST bit to define the frame format
    * Configure the CRCL and CRCEN bits if CRC is needed (while SCK clock signal is at idle state).
    * Configure SSM and SSI
    * Configure the MSTR bit (in multimaster NSS configuration, avoid conflict state on NSS if master is configured to prevent MODF error).
3. Write to SPI_CR2 register:
    * Configure the DS[3:0] bits to select the data length for the transfer.
    * Configure SSOE (Notes: 1 & 2 & 3).
    * Set the FRF bit if the TI protocol is required (keep NSSP bit cleared in TI mode).
    * Set the NSSP bit if the NSS pulse mode between two data units is required (keep CHPA and TI bits cleared in NSSP mode).
    * Configure the FRXTH bit. The RXFIFO threshold must be aligned to the read access size for the SPIx_DR register.
    * Initialize LDMA_TX and LDMA_RX bits if DMA is used in packed mode.
4. Write to SPI_CRCPR register: Configure the CRC polynomial if needed.
5. Write proper DMA registers: Configure DMA streams dedicated for SPI Tx and Rx in DMA registers if the DMA streams are used.

Notes:

* (Note 1) Step is not required in slave mode.
* (Note 2) Step is not required in TI mode.
* (Note 3) Step is not required in NSSP mode.
* (Note 4) The step is not required in slave mode except slave working at TI mode
